import { defineStore } from "pinia";
import { onScopeDispose, ref } from "vue";
import { native } from "../services/native";
import { VoiceEngine, voiceSupported } from "../services/voice";
import { speakingNow } from "../services/voiceJitter";

function fromBase64(text: string): Uint8Array {
  const bin = atob(text);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** In-room voice chat: mic on/off like Meet, and who is speaking. */
export const useVoice = defineStore("voice", () => {
  /** The microphone/engine is open (the user joined voice). */
  const active = ref(false);
  const muted = ref(true);
  const busy = ref(false);
  const error = ref("");
  /** Client ids heard in the last few hundred ms. */
  const speaking = ref<string[]>([]);

  const lastFrameAt = new Map<string, number>();
  const engine = new VoiceEngine({
    send: (frame) => {
      void native.sendVoiceFrame(frame).catch(() => {});
    },
  });
  let unlistenFrames: (() => void) | null = null;
  let unlistenToggle: (() => void) | null = null;
  let speakingTimer: ReturnType<typeof setInterval> | null = null;

  async function ensureListeners() {
    if (!unlistenFrames) {
      unlistenFrames = await native.onVoiceFrames((batch) => {
        const now = performance.now();
        for (const { from, data } of batch) {
          lastFrameAt.set(from, now);
          engine.push(from, fromBase64(data));
        }
      });
    }
    if (!speakingTimer) {
      speakingTimer = setInterval(() => {
        const next = speakingNow(lastFrameAt, performance.now());
        if (next.join() !== speaking.value.join()) speaking.value = next;
      }, 150);
    }
  }

  /** Joins voice (opens the mic, unmuted) or, once joined, flips mute. */
  async function toggle() {
    if (busy.value) return;
    error.value = "";
    if (!active.value) {
      if (!voiceSupported()) {
        error.value = "Este entorno no soporta voz (WebCodecs/Opus).";
        return;
      }
      busy.value = true;
      try {
        await ensureListeners();
        await engine.start();
        engine.setMuted(false);
        muted.value = false;
        active.value = true;
        // Mute toggle while the game has focus; a taken key is not fatal.
        await native.registerVoiceShortcut().catch((e) => {
          error.value = e instanceof Error ? e.message : String(e);
        });
      } catch (e) {
        error.value =
          e instanceof DOMException && e.name === "NotAllowedError"
            ? "Permiso de micrófono denegado."
            : e instanceof Error
              ? e.message
              : String(e);
        await leave();
      } finally {
        busy.value = false;
      }
      return;
    }
    muted.value = !muted.value;
    engine.setMuted(muted.value);
  }

  /** Hears the room again after a restart of the room session. */
  async function listenOnly() {
    await ensureListeners();
    if (!unlistenToggle) unlistenToggle = await native.onVoiceToggle(() => void toggle());
  }

  async function leave() {
    await native.unregisterVoiceShortcut().catch(() => {});
    await engine.stop();
    active.value = false;
    muted.value = true;
    speaking.value = [];
    lastFrameAt.clear();
    unlistenFrames?.();
    unlistenFrames = null;
    unlistenToggle?.();
    unlistenToggle = null;
    if (speakingTimer) clearInterval(speakingTimer);
    speakingTimer = null;
  }

  onScopeDispose(() => void leave());

  return { active, muted, busy, error, speaking, toggle, leave, listenOnly };
});
