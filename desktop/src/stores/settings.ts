import { defineStore } from "pinia";
import { ref } from "vue";
import { native } from "../services/native";

/** Local (per machine) settings kept by the desktop shell. Only the game
 * fields are edited here; everything else is preserved as loaded. */
export const useSettings = defineStore("settings-v2", () => {
  const raw = ref<Record<string, unknown>>({});
  const gameExecutablePath = ref("");
  const gameArgs = ref("");
  const voiceEnabled = ref(false);
  /** The app's own file log level and how long log files are kept. */
  const fileLogLevel = ref("info");
  const logRetentionDays = ref(7);
  const loaded = ref(false);

  async function load() {
    const loadedSettings = (await native.loadSettings()) ?? { gameExecutablePath: "", gameArgs: "" };
    raw.value = loadedSettings;
    gameExecutablePath.value = String(loadedSettings.gameExecutablePath ?? "");
    gameArgs.value = String(loadedSettings.gameArgs ?? "");
    voiceEnabled.value = Boolean(loadedSettings.voiceEnabled);
    fileLogLevel.value = String(loadedSettings.fileLogLevel ?? "info");
    logRetentionDays.value = Number(loadedSettings.logRetentionDays ?? 7);
    loaded.value = true;
  }

  async function save() {
    const next = {
      ...raw.value,
      gameExecutablePath: gameExecutablePath.value,
      gameArgs: gameArgs.value,
      voiceEnabled: voiceEnabled.value,
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

  async function setLogRetentionDays(days: number) {
    await native.setLogRetentionDays(days);
    logRetentionDays.value = days;
    raw.value = { ...raw.value, logRetentionDays: days };
  }

  return {
    gameExecutablePath,
    gameArgs,
    voiceEnabled,
    fileLogLevel,
    logRetentionDays,
    loaded,
    load,
    save,
    setFileLogLevel,
    setLogRetentionDays,
  };
});
