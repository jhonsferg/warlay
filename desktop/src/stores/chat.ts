import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { chat as api } from "../services/api";
import type { ChannelKind, ChatMessage } from "../services/types";
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

export const useChat = defineStore("chat", () => {
  const channels = ref<Record<string, Channel>>({});
  const unread = ref<Record<string, number>>({});
  /** The channel currently on screen, which never counts as unread. */
  const focused = ref<string | null>(null);
  /** DM partners with recent activity, newest first. */
  const recentDms = ref<string[]>([]);

  const totalUnreadDms = computed(() =>
    Object.entries(unread.value)
      .filter(([key]) => key.startsWith("dm:"))
      .reduce((sum, [, n]) => sum + n, 0),
  );

  function channel(key: string): Channel {
    channels.value[key] ??= { messages: [], loaded: false, hasMore: true };
    return channels.value[key];
  }

  async function load(kind: ChannelKind, id: string) {
    const key = channelKey(kind, id);
    const ch = channel(key);
    if (ch.loaded) return;
    const messages = await api.history(kind, id);
    ch.messages = messages;
    ch.loaded = true;
    ch.hasMore = messages.length >= 50;
  }

  async function loadOlder(kind: ChannelKind, id: string) {
    const ch = channel(channelKey(kind, id));
    if (!ch.hasMore || ch.messages.length === 0) return;
    const older = await api.history(kind, id, ch.messages[0].createdAt);
    ch.messages = [...older, ...ch.messages];
    ch.hasMore = older.length >= 50;
  }

  async function send(kind: ChannelKind, id: string, body: string) {
    const message = await api.send(kind, id, body);
    receive(message);
  }

  function focus(key: string | null) {
    focused.value = key;
    if (key) unread.value[key] = 0;
  }

  /** Maps the server's channel name to our key ("dm:a:b" -> "dm:<other>"). */
  function keyFor(message: ChatMessage): string {
    if (message.channel.startsWith("dm:")) {
      const myId = useSession().me?.id;
      const [, a, b] = message.channel.split(":");
      return `dm:${a === myId ? b : a}`;
    }
    return message.channel;
  }

  function receive(message: ChatMessage) {
    const key = keyFor(message);
    const ch = channel(key);
    if (ch.messages.some((m) => m.id === message.id)) return;
    ch.messages.push(message);
    const mine = message.sender.id === useSession().me?.id;
    if (!mine && focused.value !== key) unread.value[key] = (unread.value[key] ?? 0) + 1;
    if (key.startsWith("dm:")) {
      const partner = key.slice(3);
      recentDms.value = [partner, ...recentDms.value.filter((p) => p !== partner)];
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
        const latest = await api.history(kind, key.slice(separator + 1));
        const missed = latest.filter((m) => !channel(key).messages.some((known) => known.id === m.id));
        if (missed.length === 0) return;
        missed.forEach(receive);
        channel(key).messages.sort((a, b) => a.createdAt.localeCompare(b.createdAt));
      }),
    );
  }

  function reset() {
    channels.value = {};
    unread.value = {};
    recentDms.value = [];
    focused.value = null;
  }

  return {
    channels,
    unread,
    focused,
    recentDms,
    totalUnreadDms,
    channel,
    load,
    loadOlder,
    send,
    focus,
    receive,
    catchUp,
    reset,
  };
});
