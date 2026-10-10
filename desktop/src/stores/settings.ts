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
  const loaded = ref(false);

  async function load() {
    const loadedSettings = (await native.loadSettings()) ?? { gameExecutablePath: "", gameArgs: "" };
    raw.value = loadedSettings;
    gameExecutablePath.value = String(loadedSettings.gameExecutablePath ?? "");
    gameArgs.value = String(loadedSettings.gameArgs ?? "");
    voiceEnabled.value = Boolean(loadedSettings.voiceEnabled);
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

  return { gameExecutablePath, gameArgs, voiceEnabled, loaded, load, save };
});
