import { onScopeDispose, ref, type Ref } from "vue";

/**
 * A clock shared by everything that prints a relative time ("hace 2
 * minutos"): one timer for the whole app, ticking every 30 s, started with the
 * first user and stopped with the last.
 */
const now = ref(Date.now());
let users = 0;
let timer: ReturnType<typeof setInterval> | null = null;

export function useNow(): Ref<number> {
  now.value = Date.now();
  users++;
  timer ??= setInterval(() => (now.value = Date.now()), 30_000);
  onScopeDispose(() => {
    users--;
    if (users <= 0 && timer) {
      clearInterval(timer);
      timer = null;
    }
  });
  return now;
}
