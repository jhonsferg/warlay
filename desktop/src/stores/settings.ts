import { defineStore } from "pinia";
import { ref } from "vue";
import { native } from "../services/native";
import { normalizeNoiseLevel, type NoiseLevel } from "../services/voicePrefs";

/** Local (per machine) settings kept by the desktop shell. Only the game
 * fields are edited here; everything else is preserved as loaded. */
export const useSettings = defineStore("settings-v2", () => {
  const raw = ref<Record<string, unknown>>({});
  const gameExecutablePath = ref("");
  const gameArgs = ref("");
  /** The app's own file log level and how long log files are kept. */
  const fileLogLevel = ref("info");
  const logRetentionDays = ref(7);
  const loaded = ref(false);
  /** Microphone: Warlay's own consent, chosen input and noise level. */
  const micConsent = ref(false);
  const micDeviceId = ref("");
  const noiseSuppression = ref<NoiseLevel>("standard");

  async function load() {
    const loadedSettings = (await native.loadSettings()) ?? { gameExecutablePath: "", gameArgs: "" };
    raw.value = loadedSettings;
    gameExecutablePath.value = String(loadedSettings.gameExecutablePath ?? "");
    gameArgs.value = String(loadedSettings.gameArgs ?? "");
    fileLogLevel.value = String(loadedSettings.fileLogLevel ?? "info");
    logRetentionDays.value = Number(loadedSettings.logRetentionDays ?? 7);
    micConsent.value = loadedSettings.micConsent === true;
    micDeviceId.value = String(loadedSettings.micDeviceId ?? "");
    noiseSuppression.value = normalizeNoiseLevel(loadedSettings.noiseSuppression);
    loaded.value = true;
  }

  async function save() {
    const next = {
      ...raw.value,
      gameExecutablePath: gameExecutablePath.value,
      gameArgs: gameArgs.value,
    };
    await native.saveSettings(next);
    raw.value = next;
  }

  // These two apply and persist on their own (no "Guardar" click); `raw` is
  // kept in step so a later `save()` of the game fields doesn't put the old
  // values back.
  async function setFileLogLevel(level: string) {
    await native.setLogLevel("file", level);
    fileLogLevel.value = level;
    raw.value = { ...raw.value, fileLogLevel: level };
  }

  /** Applies and persists on its own, like the log levels. */
  async function setMicConsent(granted: boolean) {
    await native.setMicConsent(granted);
    micConsent.value = granted;
    raw.value = { ...raw.value, micConsent: granted };
  }

  async function setVoicePrefs(prefs: { micDeviceId: string; noiseSuppression: NoiseLevel }) {
    const next = { ...raw.value, ...prefs };
    await native.saveSettings(next);
    raw.value = next;
    micDeviceId.value = prefs.micDeviceId;
    noiseSuppression.value = prefs.noiseSuppression;
  }

  async function setLogRetentionDays(days: number) {
    await native.setLogRetentionDays(days);
    logRetentionDays.value = days;
    raw.value = { ...raw.value, logRetentionDays: days };
  }

  return {
    gameExecutablePath,
    gameArgs,
    fileLogLevel,
    logRetentionDays,
    loaded,
    micConsent,
    micDeviceId,
    noiseSuppression,
    setMicConsent,
    setVoicePrefs,
    load,
    save,
    setFileLogLevel,
    setLogRetentionDays,
  };
});
