import { defineStore } from "pinia";
import { ref } from "vue";
import { native } from "../services/native";
import { createUpdater, initialUpdateState, readLastCheck, saveLastCheck } from "../services/updates";

/** The browser's storage, when there is one (plain Node tests have none). */
const storage = typeof localStorage === "undefined" ? null : localStorage;

/** Client self-update state, shared so it survives leaving the settings page
 * (a download keeps going while the user navigates). */
export const useUpdates = defineStore("updates", () => {
  const state = ref(initialUpdateState());
  /** When the last check finished without an error (ms); null if never. */
  const lastCheckedAt = ref<number | null>(storage ? readLastCheck(storage) : null);

  const updater = createUpdater(
    {
      check: () => native.checkForUpdate(),
      install: () => native.installUpdate(),
      onProgress: (handler) => native.onUpdateProgress(handler),
    },
    (next) => {
      const finishedCheck =
        state.value.phase === "checking" && (next.phase === "available" || next.phase === "uptodate");
      state.value = next;
      if (finishedCheck) {
        lastCheckedAt.value = Date.now();
        if (storage) saveLastCheck(storage, lastCheckedAt.value);
      }
    },
  );

  return { state, lastCheckedAt, check: updater.check, install: updater.install };
});
