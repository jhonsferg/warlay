import { defineStore } from "pinia";
import { ref } from "vue";
import {
  saveSettings as apiSaveSettings,
  setLogLevel as apiSetLogLevel,
  setLogRetentionDays as apiSetLogRetentionDays,
} from "../api";
import type { AppSettings, LogLevel } from "../types";

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<AppSettings>({
    displayName: "",
    gameExecutablePath: "",
    gameArgs: "",
    gameVersion: 26,
    slotsTotal: 12,
    graphicsApi: "",
    consoleLogLevel: "debug",
    fileLogLevel: "debug",
    chatMessageLimit: 500,
    masterVolume: 1,
    muted: false,
    audioOutputDeviceId: "",
    logRetentionDays: 7,
    voiceEnabled: false,
    microphoneDeviceId: "",
    micToggleKey: "",
    voiceInputVolume: 1,
    voiceOutputVolume: 1,
  });
  const clientId = ref("");
  /** Shown on the Ajustes view when a connect attempt bounced the user
   * there for lacking a display name — never sent to the hub, since an
   * empty name silently fails app-level auth (no error surfaces where the
   * user is looking, and other clients won't see this client's rooms until
   * it successfully authenticates). */
  const pendingGuidance = ref<string | null>(null);

  async function persistSettings(next: AppSettings): Promise<void> {
    await apiSaveSettings(next);
    settings.value = next;
    if (next.displayName.trim()) pendingGuidance.value = null;
  }

  /** Applies immediately on the backend (see `log_control`) and persists —
   * the one settings control that's instant rather than autosave-on-a-delay. */
  async function setLogLevel(target: "console" | "file", level: LogLevel): Promise<void> {
    await apiSetLogLevel(target, level);
    if (target === "console") settings.value.consoleLogLevel = level;
    else settings.value.fileLogLevel = level;
  }

  /** Mutates `settings.value` first, before the save round-trip — `audio.ts`
   * reads these fields live off this store on every `playSound` call, so the
   * mute toggle / volume slider take effect on the very next sound rather
   * than after the debounced autosave elsewhere in `SettingsView` would have
   * gotten around to it. */
  async function setAudioPrefs(
    prefs: Partial<Pick<AppSettings, "masterVolume" | "muted" | "audioOutputDeviceId">>,
  ): Promise<void> {
    Object.assign(settings.value, prefs);
    await apiSaveSettings(settings.value);
  }

  async function setVoicePrefs(
    prefs: Partial<
      Pick<
        AppSettings,
        "voiceEnabled" | "microphoneDeviceId" | "micToggleKey" | "voiceInputVolume" | "voiceOutputVolume"
      >
    >,
  ): Promise<void> {
    Object.assign(settings.value, prefs);
    await apiSaveSettings(settings.value);
  }

  /** Applies immediately (re-runs the cleanup sweep on the backend) and
   * persists — same instant-feeling pattern as `setLogLevel`. */
  async function setLogRetentionDays(days: number): Promise<void> {
    await apiSetLogRetentionDays(days);
    settings.value.logRetentionDays = days;
  }

  return {
    settings,
    clientId,
    pendingGuidance,
    persistSettings,
    setLogLevel,
    setAudioPrefs,
    setVoicePrefs,
    setLogRetentionDays,
  };
});
