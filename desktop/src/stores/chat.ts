import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { chat as api, receipts as receiptsApi } from "../services/api";
import { notifyOs, windowActive } from "../services/osNotify";
import type { ChannelKind, ChatMessage, MessageStatus, ReceiptCursors } from "../services/types";
import { isEnvelope, useE2e } from "./e2e";
import { useSession } from "./session";

export interface Channel {
  messages: ChatMessage[];
  loaded: boolean;
  hasMore: boolean;
}

/** Client-side channel key: "dm:<friendId>", "room:<id>", "community:<id>". */
export function channelKey(kind: ChannelKind, id: string): string {
  return `${kind}:${id}`;
}

/** Microseconds since the epoch. The server's timestamps carry a fraction of
 * variable length ("...:13Z", "...:13.5Z"), so they can't be compared as
 * strings, and milliseconds would blur messages sent in the same one. */
export function micros(iso: string): number {
  const fraction = /\.(\d+)/.exec(iso)?.[1] ?? "";
  const ms = Date.parse(iso.replace(/\.\d+/, ""));
  return ms * 1000 + Number(fraction.padEnd(6, "0").slice(0, 6));
}

/** Waits a moment before telling the server, folding bursts into one call. */
const ACK_DELAY_MS = 300;

interface PendingAck {
  deliveredUpTo?: string;
  readUpTo?: string;
  timer: ReturnType<typeof setTimeout>;
}

export const useChat = defineStore("chat", () => {
  const channels = ref<Record<string, Channel>>({});
  const unread = ref<Record<string, number>>({});
  /** The channel currently on screen, which never counts as unread. */
  const focused = ref<string | null>(null);
  /** DM partners with recent activity, newest first. */
  const recentDms = ref<string[]>([]);
  /** How far each partner has received/read what we sent them, by partner id. */
  const peerReceipts = ref<Record<string, ReceiptCursors>>({});
  const pendingAcks = new Map<string, PendingAck>();
  /** Request id -> message id, per DM, to drop an encrypted message the server
   * replays under another id. */
  const seenRequests = new Map<string, Map<string, string>>();

  const totalUnreadDms = computed(() =>
    Object.entries(unread.value)
      .filter(([key]) => key.startsWith("dm:"))
      .reduce((sum, [, n]) => sum + n, 0),
  );

  function channel(key: string): Channel {
    channels.value[key] ??= { messages: [], loaded: false, hasMore: true };
    return channels.value[key];
  }

  const myId = () => useSession().me?.id;

  /** The partner in a server DM channel name ("dm:<low>:<high>"). */
  function partnerOf(serverChannel: string): string {
    const [, a, b] = serverChannel.split(":");
    return a === myId() ? b : a;
  }

  /** Maps the server's channel name to our key ("dm:a:b" -> "dm:<other>"). */
  function keyOfChannel(serverChannel: string): string {
    return serverChannel.startsWith("dm:") ? `dm:${partnerOf(serverChannel)}` : serverChannel;
  }

  /** Opens an end-to-end encrypted direct message. Plain ones pass through;
   * `null` is a replay of a message already shown. */
  async function openMessage(message: ChatMessage): Promise<ChatMessage | null> {
    if (!message.channel.startsWith("dm:") || !isEnvelope(message.body)) return message;
    const partner = partnerOf(message.channel);
    const opened = await useE2e().decrypt(partner, message);
    if (!opened) return { ...message, body: "", undecryptable: true };
    const key = keyOfChannel(message.channel);
    const seen = seenRequests.get(key) ?? new Map<string, string>();
    seenRequests.set(key, seen);
    const first = seen.get(opened.requestId);
    if (first !== undefined && first !== message.id) return null;
    seen.set(opened.requestId, message.id);
    return { ...message, body: opened.plaintext, encrypted: true };
  }

  async function openAll(messages: ChatMessage[]): Promise<ChatMessage[]> {
    const opened = [];
    // In order, so a replay is the later of two copies.
    for (const message of messages) {
      const result = await openMessage(message);
      if (result) opened.push(result);
    }
    return opened;
  }

  /** Newest message the partner sent us in a DM, if any is loaded. */
  function latestIncoming(key: string): ChatMessage | undefined {
    const partner = key.slice(3);
    return [...channel(key).messages].reverse().find((m) => m.sender.id === partner);
  }

  /** Tells the server we received (and, when the chat is open, read) a DM. */
  function ack(partner: string, state: "delivered" | "read", upTo: string) {
    const pending = pendingAcks.get(partner);
    if (pending) clearTimeout(pending.timer);
    const next: PendingAck = {
      deliveredUpTo: pending?.deliveredUpTo,
      readUpTo: pending?.readUpTo,
      timer: setTimeout(() => {
        pendingAcks.delete(partner);
        // Reading implies delivery up to the same point.
        if (next.readUpTo) void receiptsApi.mark(partner, "read", next.readUpTo).catch(() => {});
        if (next.deliveredUpTo && (!next.readUpTo || micros(next.deliveredUpTo) > micros(next.readUpTo))) {
          void receiptsApi.mark(partner, "delivered", next.deliveredUpTo).catch(() => {});
        }
      }, ACK_DELAY_MS),
    };
    const field = state === "read" ? "readUpTo" : "deliveredUpTo";
    const current = next[field];
    next[field] = !current || micros(upTo) > micros(current) ? upTo : current;
    pendingAcks.set(partner, next);
  }

  /** Acknowledges as read whatever the open DM has received. */
  function readOpenDm() {
    const key = focused.value;
    if (!key?.startsWith("dm:") || !windowActive()) return;
    unread.value[key] = 0;
    const latest = latestIncoming(key);
    if (latest) ack(key.slice(3), "read", latest.createdAt);
  }

  async function loadPeerReceipt(partner: string) {
    try {
      peerReceipts.value[partner] = await receiptsApi.peer(partner);
    } catch {
      // Statuses are cosmetic: without them messages simply read "sent".
    }
  }

  async function load(kind: ChannelKind, id: string) {
    const key = channelKey(kind, id);
    const ch = channel(key);
    if (ch.loaded) return;
    if (kind === "dm") await useE2e().prepare(id);
    const messages = await openAll(await api.history(kind, id));
    ch.messages = messages;
    ch.loaded = true;
    ch.hasMore = messages.length >= 50;
    if (kind === "dm") void loadPeerReceipt(id);
    // The chat was focused before its history arrived: now there is
    // something to acknowledge as read.
    if (focused.value === key) readOpenDm();
  }

  async function loadOlder(kind: ChannelKind, id: string) {
    const ch = channel(channelKey(kind, id));
    if (!ch.hasMore || ch.messages.length === 0) return;
    const older = await openAll(await api.history(kind, id, ch.messages[0].createdAt));
    ch.messages = [...older, ...ch.messages];
    ch.hasMore = older.length >= 50;
  }

  async function send(kind: ChannelKind, id: string, body: string) {
    const requestId = crypto.randomUUID();
    // Direct messages go out sealed whenever both sides have encryption.
    const sealed = kind === "dm" ? await useE2e().encrypt(id, requestId, body) : null;
    const message = await api.send(kind, id, sealed ?? body, requestId);
    await receive(message);
  }

  /** Deletes one of our own messages for everybody. */
  async function remove(message: ChatMessage) {
    const key = keyOfChannel(message.channel);
    const separator = key.indexOf(":");
    await api.remove(key.slice(0, separator) as ChannelKind, key.slice(separator + 1), message.id);
    applyDeleted(message.channel, message.id);
  }

  function focus(key: string | null) {
    focused.value = key;
    if (key) unread.value[key] = 0;
    readOpenDm();
  }

  async function receive(received: ChatMessage) {
    const message = await openMessage(received);
    if (!message) return;
    const key = keyOfChannel(message.channel);
    const ch = channel(key);
    if (ch.messages.some((m) => m.id === message.id)) return;
    ch.messages.push(message);
    const mine = message.sender.id === myId();
    const open = focused.value === key && windowActive();
    if (!mine && (focused.value !== key || !open)) unread.value[key] = (unread.value[key] ?? 0) + 1;
    if (key.startsWith("dm:")) {
      const partner = key.slice(3);
      recentDms.value = [partner, ...recentDms.value.filter((p) => p !== partner)];
      if (!mine) {
        ack(partner, open ? "read" : "delivered", message.createdAt);
        void notifyOs(message.sender.displayName, message.body);
      }
    }
  }

  /** A message was deleted (by us, here or on another device, or by its sender). */
  function applyDeleted(serverChannel: string, messageId: string) {
    const key = keyOfChannel(serverChannel);
    const ch = channels.value[key];
    if (!ch) return;
    const deleted = ch.messages.find((m) => m.id === messageId);
    ch.messages = ch.messages.filter((m) => m.id !== messageId);
    // The server knows what is still unread; ours may have counted this one.
    if (deleted && deleted.sender.id !== myId() && (unread.value[key] ?? 0) > 0) void loadUnread();
  }

  /** A receipt arrived: the partner's cursors for what we sent, or our own
   * from another device (which clears our badge there too). */
  function applyReceipt(serverChannel: string, userId: string, receipt: ReceiptCursors) {
    const key = keyOfChannel(serverChannel);
    if (!key.startsWith("dm:")) return;
    if (userId === myId()) {
      const partner = key.slice(3);
      const after = channel(key).messages.filter(
        (m) => m.sender.id === partner && micros(m.createdAt) > micros(receipt.readAt),
      );
      if (focused.value !== key) unread.value[key] = Math.min(unread.value[key] ?? 0, after.length);
      return;
    }
    const known = peerReceipts.value[userId];
    peerReceipts.value[userId] = {
      deliveredAt:
        known && micros(known.deliveredAt) > micros(receipt.deliveredAt) ? known.deliveredAt : receipt.deliveredAt,
      readAt: known && micros(known.readAt) > micros(receipt.readAt) ? known.readAt : receipt.readAt,
    };
  }

  /** What the sender sees under one of their own direct messages. */
  function statusOf(message: ChatMessage): MessageStatus {
    if (!message.channel.startsWith("dm:")) return "sent";
    const cursors = peerReceipts.value[partnerOf(message.channel)];
    if (!cursors) return "sent";
    const at = micros(message.createdAt);
    if (at <= micros(cursors.readAt)) return "read";
    if (at <= micros(cursors.deliveredAt)) return "delivered";
    return "sent";
  }

  /** Unread DMs live on the server, so they survive restarts and follow the
   * user across devices; also reports everything pending as delivered. */
  async function loadUnread() {
    const list = await receiptsApi.unread();
    const pending = new Set(list.map((u) => `dm:${u.peerId}`));
    for (const key of Object.keys(unread.value)) {
      if (key.startsWith("dm:") && !pending.has(key)) unread.value[key] = 0;
    }
    for (const entry of list) {
      const key = `dm:${entry.peerId}`;
      unread.value[key] = focused.value === key ? 0 : entry.count;
      ack(entry.peerId, "delivered", entry.latestAt);
    }
  }

  /** After the realtime socket comes back: messages pushed while it was
   * down never arrived, so top up every channel already on screen from its
   * latest page of history. */
  async function catchUp() {
    const loaded = Object.entries(channels.value).filter(([, ch]) => ch.loaded);
    await Promise.allSettled(
      loaded.map(async ([key]) => {
        const separator = key.indexOf(":");
        const kind = key.slice(0, separator) as ChannelKind;
        const latest = await openAll(await api.history(kind, key.slice(separator + 1)));
        // What was deleted while we were away must go too.
        const alive = new Set(latest.map((m) => m.id));
        const oldest = latest[0] ? micros(latest[0].createdAt) : Infinity;
        channel(key).messages = channel(key).messages.filter((m) => alive.has(m.id) || micros(m.createdAt) < oldest);
        if (kind === "dm") void loadPeerReceipt(key.slice(separator + 1));
        const missed = latest.filter((m) => !channel(key).messages.some((known) => known.id === m.id));
        if (missed.length === 0) return;
        await Promise.all(missed.map(receive));
        channel(key).messages.sort((a, b) => micros(a.createdAt) - micros(b.createdAt));
      }),
    );
  }

  function reset() {
    pendingAcks.forEach((pending) => clearTimeout(pending.timer));
    pendingAcks.clear();
    seenRequests.clear();
    channels.value = {};
    unread.value = {};
    recentDms.value = [];
    peerReceipts.value = {};
    focused.value = null;
  }

  // Coming back to the window is what "reads" the open conversation.
  window.addEventListener("focus", readOpenDm);
  document.addEventListener("visibilitychange", readOpenDm);

  return {
    channels,
    unread,
    focused,
    recentDms,
    peerReceipts,
    totalUnreadDms,
    channel,
    load,
    loadOlder,
    send,
    remove,
    focus,
    receive,
    applyDeleted,
    applyReceipt,
    statusOf,
    loadUnread,
    catchUp,
    reset,
  };
});
