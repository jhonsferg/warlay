import { defineStore } from "pinia";
import { onScopeDispose, ref } from "vue";
import { native } from "../services/native";
import { VoiceEngine, voiceSupported } from "../services/voice";
import { speakingNow } from "../services/voiceJitter";
import { classifyMicError } from "../services/voicePrefs";
import { useSettings } from "./settings";
import { useToasts } from "./toasts";

function fromBase64(text: string): Uint8Array {
  const bin = atob(text);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Which microphone dialog is showing: Warlay's own request, or the guide
 * for when Windows itself blocks the microphone. */
export type MicDialog = "ask" | "blocked" | null;

/** In-room voice chat: mic on/off like Meet, and who is speaking. */
export const useVoice = defineStore("voice", () => {
  const settings = useSettings();
  const toasts = useToasts();
  /** The microphone/engine is open (the user joined voice). */
  const active = ref(false);
  const muted = ref(true);
  const busy = ref(false);
  const error = ref("");
  const dialog = ref<MicDialog>(null);
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
  let consentResolver: ((granted: boolean) => void) | null = null;

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

  /** Shows Warlay's own request and resolves with the user's answer. */
  function askConsent(): Promise<boolean> {
    dialog.value = "ask";
    return new Promise((resolve) => {
      consentResolver = resolve;
    });
  }

  /** The dialog's buttons. */
  function answerConsent(granted: boolean) {
    dialog.value = null;
    consentResolver?.(granted);
    consentResolver = null;
  }

  function dismissDialog() {
    if (dialog.value === "ask") answerConsent(false);
    else dialog.value = null;
  }

  /** Joins voice (opens the mic, unmuted) or, once joined, flips mute. */
  async function toggle() {
    if (busy.value) return;
    error.value = "";
    if (active.value) {
      muted.value = !muted.value;
      engine.setMuted(muted.value);
      return;
    }
    if (!voiceSupported()) {
      error.value = "Este entorno no soporta voz (WebCodecs/Opus).";
      return;
    }
    busy.value = true;
    try {
      if (!settings.loaded) await settings.load();
      // First time: Warlay asks, instead of the engine's default prompt. The
      // answer is stored BEFORE getUserMedia -- the native permission policy
      // reads it.
      if (!settings.micConsent) {
        if (!(await askConsent())) {
          error.value = classifyMicError({ name: "NotAllowedError" }, false).message;
          return;
        }
        await settings.setMicConsent(true);
      }
      await ensureListeners();
      await engine.start({ deviceId: settings.micDeviceId, noise: settings.noiseSuppression });
      engine.setMuted(false);
      muted.value = false;
      active.value = true;
      if (settings.noiseSuppression === "strong" && engine.noiseFallbackReason) {
        toasts.info("Supresión fuerte no disponible", "Se usa la estándar del navegador.");
      }
      // Mute toggle while the game has focus; a taken key is not fatal.
      await native.registerVoiceShortcut().catch((e) => {
        error.value = e instanceof Error ? e.message : String(e);
      });
    } catch (e) {
      const info = classifyMicError(e, settings.micConsent);
      error.value = info.message;
      if (info.kind === "blocked-by-os") dialog.value = "blocked";
      await leave();
    } finally {
      busy.value = false;
    }
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
    // A failing unlisten must never leave voice half torn down.
    for (const stop of [unlistenFrames, unlistenToggle]) {
      try {
        stop?.();
      } catch {
        /* already gone */
      }
    }
    unlistenFrames = null;
    unlistenToggle = null;
    if (speakingTimer) clearInterval(speakingTimer);
    speakingTimer = null;
  }

  onScopeDispose(() => void leave());

  return {
    active,
    muted,
    busy,
    error,
    dialog,
    speaking,
    toggle,
    leave,
    listenOnly,
    answerConsent,
    dismissDialog,
  };
});
