import { defineStore } from "pinia";
import { ref } from "vue";

/** Whether the configured game executable is currently running, per the
 * backend's own OS-level check (`is_process_running_at` in
 * `desktop-tauri/src/launcher/process_launcher.rs`) — reflects reality
 * regardless of whether this app is the one that launched it. Seeded once
 * at `bootstrap()` time and kept live via `game://running-state-changed`
 * (see `api.ts`). */
export const useGameProcessStore = defineStore("gameProcess", () => {
  const running = ref(false);

  return { running };
});
