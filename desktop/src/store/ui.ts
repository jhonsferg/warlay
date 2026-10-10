import { defineStore } from "pinia";
import { ref } from "vue";

/** What the main content area currently shows — servers live in the left
 * rail, not a tab. `null` only before the first server connects (or
 * forever, on a fresh install with no servers registered yet) — see
 * `EmptyState.vue`. */
export type RailSelection =
  | { kind: "server"; serverId: string }
  | { kind: "lan" }
  | { kind: "settings" }
  | { kind: "agent" }
  | null;

/** Purely "what's currently shown" — doesn't belong to any one domain store,
 * so it gets its own. */
export const useUiStore = defineStore("ui", () => {
  const railSelection = ref<RailSelection>(null);
  const showExitBanner = ref(false);
  const consoleDrawerOpen = ref(false);
  /** Gates the whole shell -- see App.vue's daemon-gate screen. False until
   * the daemon is confirmed up and answering. */
  const daemonReady = ref(false);

  function toggleConsoleDrawer(): void {
    consoleDrawerOpen.value = !consoleDrawerOpen.value;
  }

  function closeConsoleDrawer(): void {
    consoleDrawerOpen.value = false;
  }

  return { railSelection, showExitBanner, consoleDrawerOpen, daemonReady, toggleConsoleDrawer, closeConsoleDrawer };
});
