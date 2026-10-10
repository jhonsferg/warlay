import { onMounted, onUnmounted, ref } from "vue";

// A shared, ref-counted clock rather than one setInterval per component —
// several room list items (plus the active-room panel) may all want a live
// "activa desde…" duration at once, and there's no reason each should run
// its own timer for the exact same tick.
const nowSecs = ref(Math.floor(Date.now() / 1000));
let refCount = 0;
let handle: ReturnType<typeof setInterval> | undefined;

export function useTicker() {
  onMounted(() => {
    if (refCount++ === 0) {
      handle = setInterval(() => {
        nowSecs.value = Math.floor(Date.now() / 1000);
      }, 1000);
    }
  });
  onUnmounted(() => {
    if (--refCount === 0 && handle !== undefined) {
      clearInterval(handle);
      handle = undefined;
    }
  });
  return nowSecs;
}
