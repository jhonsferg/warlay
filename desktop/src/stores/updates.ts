import { defineStore } from "pinia";
import { ref } from "vue";
import { native } from "../services/native";
import { createUpdater, initialUpdateState } from "../services/updates";

/** Client self-update state, shared so it survives leaving the settings page
 * (a download keeps going while the user navigates). */
export const useUpdates = defineStore("updates", () => {
  const state = ref(initialUpdateState());

  const updater = createUpdater(
    {
      check: () => native.checkForUpdate(),
      install: () => native.installUpdate(),
      onProgress: (handler) => native.onUpdateProgress(handler),
    },
    (next) => {
      state.value = next;
    },
  );

  return { state, check: updater.check, install: updater.install };
});
