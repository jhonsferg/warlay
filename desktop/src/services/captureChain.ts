import type { NoiseLevel } from "./voicePrefs";

/**
 * Which track feeds the Opus encoder. At the "strong" level the microphone
 * goes through a noise filter first; if that filter cannot start (worklet or
 * WASM blocked, unsupported engine) voice must keep working on the raw track
 * rather than fail -- a noisy mic beats no mic.
 *
 * Generic over the track type and free of DOM access so it is unit tested.
 */
export interface CaptureFilter<T> {
  track: T;
  dispose(): void;
}

export interface CaptureChoice<T> {
  track: T;
  /** True when `track` is the noise-filtered one. */
  filtered: boolean;
  /** Why the requested filter was not applied, if it was not. */
  fallbackReason?: string;
  dispose(): void;
}

export async function chooseCaptureTrack<T>(
  level: NoiseLevel,
  raw: T,
  createFilter: () => Promise<CaptureFilter<T>>,
): Promise<CaptureChoice<T>> {
  // Only "strong" filters (see `usesRnnoise` in voicePrefs).
  if (level !== "strong") return { track: raw, filtered: false, dispose() {} };
  try {
    const filter = await createFilter();
    return { track: filter.track, filtered: true, dispose: () => filter.dispose() };
  } catch (error) {
    return {
      track: raw,
      filtered: false,
      fallbackReason: error instanceof Error ? error.message : String(error),
      dispose() {},
    };
  }
}
