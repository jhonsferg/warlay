import { defineStore } from "pinia";
import { ref } from "vue";
import { keys } from "../services/api";
import { ApiError } from "../services/http";
import { isDesktop, native } from "../services/native";
import type { ChatMessage } from "../services/types";
import { useSession } from "./session";

/** What an encrypted message body starts with (see `warlay::crypto::e2e`). */
export const ENVELOPE_PREFIX = "e2e1:";
export const isEnvelope = (body: string) => body.startsWith(ENVELOPE_PREFIX);

/** How a conversation is protected, as the chat shows it. */
export type ConversationSecurity =
  /** This device has no encryption (a browser, or the key couldn't be set up). */
  | "off"
  /** The friend has no key yet: messages travel readable by the server. */
  | "plain"
  | "encrypted"
  /** The friend's key is not the one trusted before: it must be confirmed. */
  | "changed";

/** Raised when sending to a friend whose key changed and wasn't confirmed. */
export class KeyChangedError extends Error {
  constructor() {
    super("La clave de seguridad de tu amigo cambió. Verifícala y confíala para seguir escribiendo.");
  }
}

/**
 * End-to-end encryption of direct messages. The private key never leaves
 * the desktop shell; this store keeps the friends' public keys (pinned on
 * first use, so a key swapped later is noticed) and asks the shell to seal
 * and open messages.
 */
export const useE2e = defineStore("e2e", () => {
  const ready = ref(false);
  const myKey = ref<string | null>(null);
  /** Each friend's trusted public key (`null`: they have none). */
  const peerKeys = ref<Record<string, string | null>>({});
  /** A friend's newer key waiting for the user to confirm it. */
  const changed = ref<Record<string, string>>({});
  const safetyNumbers = ref<Record<string, string>>({});
  const resolving = new Map<string, Promise<string | null>>();
  /** Everything waits for this: a message can arrive before the key is set up. */
  let starting: Promise<void> = Promise.resolve();

  const myId = () => useSession().me?.id ?? "";
  const pinsKey = () => `warlay.e2e.pins.${myId()}`;

  function loadPins(): Record<string, string> {
    try {
      return JSON.parse(localStorage.getItem(pinsKey()) ?? "{}");
    } catch {
      return {};
    }
  }

  function savePins(pins: Record<string, string>) {
    try {
      localStorage.setItem(pinsKey(), JSON.stringify(pins));
    } catch {
      /* without storage a key is simply trusted again next time */
    }
  }

  /** Creates this device's key (once) and publishes it for friends to find. */
  function init(): Promise<void> {
    starting = setUp();
    return starting;
  }

  async function setUp() {
    ready.value = false;
    if (!isDesktop || !myId()) return;
    try {
      const publicKey = await native.e2eIdentity(myId());
      if (!publicKey) return;
      await keys.publish(publicKey);
      myKey.value = publicKey;
      ready.value = true;
    } catch {
      // Chats keep working, just not encrypted, this session.
    }
  }

  async function fetchPeerKey(peerId: string): Promise<string | null> {
    const pins = loadPins();
    let current: string;
    try {
      current = (await keys.get(peerId)).publicKey;
    } catch (error) {
      if (error instanceof ApiError && error.status === 404) {
        peerKeys.value[peerId] = null;
        return null;
      }
      // Offline or the server is down: a key seen before still works.
      return pins[peerId] ?? null;
    }
    if (!pins[peerId]) {
      pins[peerId] = current;
      savePins(pins);
    } else if (pins[peerId] !== current) {
      changed.value[peerId] = current;
    } else {
      delete changed.value[peerId];
    }
    peerKeys.value[peerId] = pins[peerId];
    return pins[peerId];
  }

  /** A friend's trusted key, fetched once and remembered. */
  async function peerKey(peerId: string): Promise<string | null> {
    await starting;
    if (!ready.value) return null;
    if (peerId in peerKeys.value) return peerKeys.value[peerId];
    let pending = resolving.get(peerId);
    if (!pending) {
      pending = fetchPeerKey(peerId).finally(() => resolving.delete(peerId));
      resolving.set(peerId, pending);
    }
    return pending;
  }

  async function computeSafetyNumber(peerId: string) {
    const key = changed.value[peerId] ?? peerKeys.value[peerId];
    if (!key) return;
    try {
      safetyNumbers.value[peerId] = await native.e2eSafetyNumber(myId(), key);
    } catch {
      /* the code is only a convenience */
    }
  }

  /** Called when a conversation opens: re-reads the friend's key (it may have
   * been published, or replaced, since) and works out the security code. */
  async function prepare(peerId: string) {
    await starting;
    if (!ready.value) return;
    delete peerKeys.value[peerId];
    await peerKey(peerId);
    await computeSafetyNumber(peerId);
  }

  /** Accepts a friend's new key as the trusted one. */
  async function trust(peerId: string) {
    const key = changed.value[peerId];
    if (!key) return;
    const pins = loadPins();
    pins[peerId] = key;
    savePins(pins);
    peerKeys.value[peerId] = key;
    delete changed.value[peerId];
    await computeSafetyNumber(peerId);
  }

  function securityOf(peerId: string): ConversationSecurity {
    if (!ready.value) return "off";
    if (changed.value[peerId]) return "changed";
    return peerKeys.value[peerId] ? "encrypted" : "plain";
  }

  /** The body to post for `text`, or `null` to send it as plain text. */
  async function encrypt(peerId: string, requestId: string, text: string): Promise<string | null> {
    await starting;
    if (!ready.value) return null;
    const key = await peerKey(peerId);
    if (changed.value[peerId]) throw new KeyChangedError();
    if (!key) return null;
    return native.e2eEncrypt(myId(), peerId, key, requestId, text);
  }

  /** Opens an encrypted message of the conversation with `peerId`; `null` if
   * this device can't (another device's key, or it was tampered with). */
  async function decrypt(
    peerId: string,
    message: ChatMessage,
  ): Promise<{ plaintext: string; requestId: string } | null> {
    await starting;
    if (!ready.value || !myKey.value) return null;
    const mine = message.sender.id === myId();
    // For our own messages the peer's key is not used, but must be well formed.
    const candidates = mine ? [myKey.value] : [await peerKey(peerId), changed.value[peerId]];
    for (const key of candidates) {
      if (!key) continue;
      try {
        return await native.e2eDecrypt(myId(), peerId, key, message.sender.id, message.body);
      } catch {
        /* try the next key, if any */
      }
    }
    return null;
  }

  function reset() {
    starting = Promise.resolve();
    ready.value = false;
    myKey.value = null;
    peerKeys.value = {};
    changed.value = {};
    safetyNumbers.value = {};
    resolving.clear();
  }

  return {
    ready,
    changed,
    safetyNumbers,
    init,
    prepare,
    trust,
    securityOf,
    encrypt,
    decrypt,
    reset,
  };
});
