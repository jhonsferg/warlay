/**
 * Playback scheduling for one remote speaker. Decoded 20 ms chunks arrive
 * with network jitter; each one is scheduled right after the previous so the
 * audio is continuous, with a small cushion so a late packet doesn't glitch.
 * The cushion follows the measured jitter (a steady link needs little, a
 * jittery one more), and a backlog beyond it is dropped so a burst never
 * leaves the speaker lagging behind for the rest of their turn.
 * Pure (no AudioContext) so it can be unit tested.
 */
/** Cushion used until enough chunks have been seen to measure the jitter. */
export const CUSHION_S = 0.06;
export const MIN_CUSHION_S = 0.04;
export const MAX_CUSHION_S = 0.12;
/** How far past the cushion playback may run ahead before a chunk is dropped. */
export const MAX_EXCESS_S = 0.08;
/** The furthest ahead playback can ever be scheduled (cushion at its maximum). */
export const MAX_LEAD_S = MAX_CUSHION_S + MAX_EXCESS_S;
/** Chunks needed before the measured jitter replaces the default cushion. */
const JITTER_SAMPLES = 5;
/** A silence this long ends a turn: it says nothing about the network. */
const TURN_GAP_S = 0.5;

/**
 * Follows how unevenly a speaker's chunks arrive (an RFC 3550-style estimate)
 * and turns it into the cushion playback should keep.
 */
export class JitterEstimator {
  private jitter = 0;
  private samples = 0;
  private lastArrival: number | null = null;
  private lastDuration = 0;
  private current = CUSHION_S;

  /** A chunk of `duration` seconds arrived at audio-clock time `now`. */
  observe(now: number, duration: number) {
    if (this.lastArrival !== null && now - this.lastArrival < TURN_GAP_S) {
      const deviation = Math.abs(now - this.lastArrival - this.lastDuration);
      this.jitter += (deviation - this.jitter) / 16;
      this.samples += 1;
      if (this.samples >= JITTER_SAMPLES) {
        this.current = Math.min(MAX_CUSHION_S, Math.max(MIN_CUSHION_S, 0.02 + 3 * this.jitter));
      }
    }
    this.lastArrival = now;
    this.lastDuration = duration;
  }

  /** The cushion to keep, in seconds. */
  get cushion(): number {
    return this.current;
  }

  reset() {
    this.jitter = 0;
    this.samples = 0;
    this.lastArrival = null;
    this.current = CUSHION_S;
  }
}

export class PlaybackClock {
  private next = 0;
  private readonly estimator = new JitterEstimator();

  /** The cushion currently in use, in seconds. */
  get currentCushion(): number {
    return this.estimator.cushion;
  }

  /**
   * @param now the audio clock (AudioContext.currentTime)
   * @param duration the chunk's length in seconds
   * @returns when to start it, or null if it should be dropped (too far ahead)
   */
  schedule(now: number, duration: number): number | null {
    this.estimator.observe(now, duration);
    const cushion = this.estimator.cushion;
    if (this.next < now) {
      // Underrun (or first chunk): restart with a cushion.
      this.next = now + cushion;
    } else if (this.next - now > cushion + MAX_EXCESS_S) {
      return null;
    }
    const start = this.next;
    this.next += duration;
    return start;
  }

  reset() {
    this.next = 0;
    this.estimator.reset();
  }
}

/** Members whose last frame arrived within `windowMs` of `now` are speaking. */
export function speakingNow(lastFrameAt: Map<string, number>, now: number, windowMs = 300): string[] {
  const out: string[] = [];
  for (const [id, at] of lastFrameAt) if (now - at <= windowMs) out.push(id);
  return out;
}
