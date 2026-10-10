import { defineStore } from "pinia";
import { ref } from "vue";

const MAX_CONSOLE_LINES = 1000;

/** Live tail of every backend log line — the console drawer's mini
 * terminal. Capped at `MAX_CONSOLE_LINES` so a long session can't grow this
 * unboundedly. */
export const useConsoleStore = defineStore("console", () => {
  const consoleLines = ref<string[]>([]);

  function pushLine(line: string): void {
    consoleLines.value.push(line.replace(/\n+$/, ""));
    if (consoleLines.value.length > MAX_CONSOLE_LINES) {
      consoleLines.value.splice(0, consoleLines.value.length - MAX_CONSOLE_LINES);
    }
  }

  function clearConsole(): void {
    consoleLines.value = [];
  }

  return { consoleLines, pushLine, clearConsole };
});
