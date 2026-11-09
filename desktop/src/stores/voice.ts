import { defineStore } from "pinia";
import { onScopeDispose, ref } from "vue";
import { native } from "../services/native";
import { VoiceEngine, voiceSupported } from "../services/voice";
import { meterLevel, smoothLevel } from "../services/voiceActivity";
import { speakingNow } from "../services/voiceJitter";
import {
  decodeStateFrame,
  encodeStateFrame,
  HEARTBEAT_MS,
  VoicePresence,
  type VoiceAnnouncement,
  type VoiceState,
} from "../services/voicePresence";
import { classifyMicError } from "../services/voicePrefs";
import { useSession } from "./session";
import { useSettings } from "./settings";
import { useToasts } from "./toasts";

/** Which microphone dialog is showing: Warlay's own request, or the guide
 * for when Windows itself blocks the microphone. */
export type MicDialog = "ask" | "blocked" | null;

/** In-room voice chat: mic on/off like Meet, and who is speaking. */
export const useVoice = defineStore("voice", () => {
  const settings = useSettings();
  const session = useSession();
  const toasts = useToasts();
  /** The microphone/engine is open (the user joined voice). */
  const active = ref(false);
  const muted = ref(true);
  const busy = ref(false);
  const error = ref("");
  const dialog = ref<MicDialog>(null);
  /** Client ids heard in the last few hundred ms. */
  const speaking = ref<string[]>([]);
  /** This PC's sound is being shared with the room right now. */
  const sharingDesktop = ref(false);

  /** Smoothed 0..1 volume of whoever is speaking, for the rings' size. */
  const levels = ref<Record<string, number>>({});

  /** Who is in voice and whether their mic is muted (the mic icons). Members
   * not listed are not in voice. Includes this user. */
  const members = ref<Record<string, VoiceState>>({});
  const presence = new VoicePresence();
  let heartbeat: ReturnType<typeof setInterval> | null = null;

  /** Tells the room this user's state (a control packet on the voice channel). */
  function announce(what: VoiceAnnouncement) {
    void native.sendVoiceFrame(encodeStateFrame(what)).catch(() => {});
  }
  function announceState() {
    announce(muted.value ? "muted" : "live");
  }
  /** Sent twice: a lost datagram would leave a stale icon until the heartbeat. */
  function announceChange() {
    announceState();
    setTimeout(() => active.value && announceState(), 120);
  }

  const lastFrameAt = new Map<string, number>();
  const rawLevel = new Map<string, number>();
  const shownLevel = new Map<string, number>();

  /** A voice was just heard: the ring lights up at once, not on the next tick. */
  function heard(id: string, level: number) {
    lastFrameAt.set(id, performance.now());
    rawLevel.set(id, Math.max(rawLevel.get(id) ?? 0, meterLevel(level)));
    if (!speaking.value.includes(id)) speaking.value = [...speaking.value, id];
  }

  const engine = new VoiceEngine({
    send: (frame) => {
      void native.sendVoiceFrame(frame).catch(() => {});
    },
    onLevel: (from, level) => {
      const id = from ?? session.me?.id;
      if (id) heard(id, level);
    },
  });
  let unlistenFrames: (() => void) | null = null;
  let unlistenDesktop: (() => void) | null = null;
  let unlistenToggle: (() => void) | null = null;
  let speakingTimer: ReturnType<typeof setInterval> | null = null;
  let consentResolver: ((granted: boolean) => void) | null = null;

  async function ensureListeners() {
    if (!unlistenFrames) {
      unlistenFrames = await native.onVoiceFrames((batch) => {
        const now = performance.now();
        for (const { from, data: bytes } of batch) {
          const control = decodeStateFrame(bytes);
          if (control) {
            if (control === "left") presence.forget(from);
            else presence.update(from, control, now);
            continue;
          }
          // The ring lights up on arrival; its size follows the decoded level.
          lastFrameAt.set(from, now);
          if (!speaking.value.includes(from)) speaking.value = [...speaking.value, from];
          engine.push(from, bytes);
        }
      });
    }
    if (!unlistenDesktop) {
      unlistenDesktop = await native.onDesktopFrames((batch) => {
        for (const { from, data } of batch) engine.pushDesktop(from, data);
      });
    }
    if (!speakingTimer) {
      speakingTimer = setInterval(() => {
        const next = speakingNow(lastFrameAt, performance.now());
        if (next.join() !== speaking.value.join()) speaking.value = next;
        // Meter: rises at once, falls gradually (like Meet and Teams).
        const out: Record<string, number> = {};
        for (const id of next) {
          const level = smoothLevel(shownLevel.get(id) ?? 0, rawLevel.get(id) ?? 0);
          shownLevel.set(id, level);
          out[id] = Math.round(level * 100) / 100;
        }
        for (const id of [...shownLevel.keys()]) if (!next.includes(id)) shownLevel.delete(id);
        rawLevel.clear();
        levels.value = out;
        const mine = session.me?.id;
        const nextMembers = presence.snapshot(performance.now());
        if (mine && active.value) nextMembers[mine] = muted.value ? "muted" : "live";
        if (JSON.stringify(nextMembers) !== JSON.stringify(members.value)) members.value = nextMembers;
      }, 60);
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

  /**
   * Starts or stops sharing the PC's sound to match `wanted`. Only while in
   * voice. A failure (old Windows, no output device, no room) tells the user
   * and switches the setting back off, so the switch never lies.
   */
  async function syncDesktopShare(wanted: boolean) {
    if (wanted && active.value && !sharingDesktop.value) {
      try {
        await native.startDesktopAudio();
        sharingDesktop.value = true;
      } catch (e) {
        toasts.error("No se pudo compartir el audio del escritorio", e instanceof Error ? e.message : String(e));
        await settings
          .setVoicePrefs({
            micDeviceId: settings.micDeviceId,
            noiseSuppression: settings.noiseSuppression,
            shareDesktopAudio: false,
            lowLatencyVoice: settings.lowLatencyVoice,
          })
          .catch(() => {});
      }
    } else if ((!wanted || !active.value) && sharingDesktop.value) {
      await native.stopDesktopAudio().catch(() => {});
      sharingDesktop.value = false;
    }
  }

  /** Joins voice (opens the mic, unmuted) or, once joined, flips mute. */
  async function toggle() {
    if (busy.value) return;
    error.value = "";
    if (active.value) {
      muted.value = !muted.value;
      engine.setMuted(muted.value);
      announceChange();
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
      await engine.start({
        deviceId: settings.micDeviceId,
        noise: settings.noiseSuppression,
        lowLatency: settings.lowLatencyVoice,
      });
      engine.setMuted(false);
      muted.value = false;
      active.value = true;
      announceChange();
      heartbeat ??= setInterval(announceState, HEARTBEAT_MS);
      if (settings.noiseSuppression === "strong" && engine.noiseFallbackReason) {
        toasts.info("Supresión fuerte no disponible", "Se usa la estándar del navegador.");
      }
      // The PC's sound is independent of the microphone's mute.
      await syncDesktopShare(settings.shareDesktopAudio);
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
    if (active.value) announce("left");
    if (heartbeat) clearInterval(heartbeat);
    heartbeat = null;
    if (sharingDesktop.value) {
      await native.stopDesktopAudio().catch(() => {});
      sharingDesktop.value = false;
    }
    await native.unregisterVoiceShortcut().catch(() => {});
    await engine.stop();
    active.value = false;
    muted.value = true;
    speaking.value = [];
    levels.value = {};
    members.value = {};
    presence.clear();
    lastFrameAt.clear();
    rawLevel.clear();
    shownLevel.clear();
    // A failing unlisten must never leave voice half torn down.
    for (const stop of [unlistenFrames, unlistenDesktop, unlistenToggle]) {
      try {
        stop?.();
      } catch {
        /* already gone */
      }
    }
    unlistenFrames = null;
    unlistenDesktop = null;
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
    levels,
    members,
    sharingDesktop,
    syncDesktopShare,
    toggle,
    leave,
    listenOnly,
    answerConsent,
    dismissDialog,
  };
});
