/**
 * Voice activity: telling speech from an open but silent microphone.
 *
 * An unmuted mic produces a frame every 20 ms whether anyone is talking or
 * not, and every frame that is sent makes the receivers show that member as
 * speaking. So the engine only sends frames while the gate is open: the level
 * is above `OPEN_LEVEL`, plus a short hangover so the end of a word (and the
 * pauses between words) is not clipped. Pure (no audio APIs) so it can be
 * unit tested.
 */

/** Frames last 20 ms; this keeps the gate open for 240 ms after the last loud one. */
export const HANGOVER_FRAMES = 12;
/** RMS (0..1) from which a frame counts as speech: about -38 dBFS, above the
 * floor the browser's noise suppression leaves and below normal speech. */
export const OPEN_LEVEL = 0.012;

/** Root-mean-square level of `samples` (0 for none). */
export function rms(samples: ArrayLike<number>): number {
  if (samples.length === 0) return 0;
  let sum = 0;
  for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];
  return Math.sqrt(sum / samples.length);
}

export class VoiceGate {
  private hold = 0;

  /** Feeds one frame's level; returns whether that frame should be sent. */
  pass(level: number): boolean {
    if (level >= OPEN_LEVEL) {
      this.hold = HANGOVER_FRAMES;
      return true;
    }
    if (this.hold > 0) {
      this.hold -= 1;
      return true;
    }
    return false;
  }

  /** Forgets any hangover (mic muted or stopped). */
  reset() {
    this.hold = 0;
  }
}
