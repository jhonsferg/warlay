export function formatSecondsAgo(secs: number): string {
  if (secs < 1) return "justo ahora";
  if (secs < 60) return `hace ${secs}s`;
  return `hace ${Math.floor(secs / 60)}m ${secs % 60}s`;
}

export function debounce<Args extends unknown[]>(fn: (...args: Args) => void, waitMs: number) {
  let handle: number | undefined;
  return (...args: Args) => {
    window.clearTimeout(handle);
    handle = window.setTimeout(() => fn(...args), waitMs);
  };
}
