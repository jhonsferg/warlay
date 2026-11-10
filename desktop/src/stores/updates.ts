import { t } from "../services/i18n";
import { defineStore } from "pinia";
import { ref } from "vue";
import { native } from "../services/native";
import { createUpdater, initialUpdateState, readLastCheck, saveLastCheck } from "../services/updates";
import { useSession } from "./session";
import { useToasts } from "./toasts";

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

/**
 * Looks for a release in the background once the app is up and, when there is
 * one, offers it in a toast. Failures stay silent: the user did not ask.
 * `goToUpdates` opens the screen with the release details.
 */
export async function announceUpdateOnStart(goToUpdates: () => unknown): Promise<void> {
  const updates = useUpdates();
  await updates.check();
  const status = updates.state.status;
  if (updates.state.phase !== "available" || !status) return;
  // The player may have signed out while the check ran: notices are for a session.
  if (useSession().status !== "signedIn") return;
  useToasts().action(
    t("Hay una nueva versión: {version}", { version: status.latest }),
    t("Para poder actualizarla haz clic aquí."),
    [
      { label: t("Ir a actualizaciones"), variant: "success", run: goToUpdates },
      { label: t("Lo haré después"), variant: "secondary", run: () => {} },
    ],
    30000,
  );
}
