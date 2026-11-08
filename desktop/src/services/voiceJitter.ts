/**
 * Playback scheduling for one remote speaker. Decoded 20 ms chunks arrive
 * with network jitter; each one is scheduled right after the previous so the
 * audio is continuous, with a small cushion so a late packet doesn't glitch.
 * Pure (no AudioContext) so it can be unit tested.
 */
export const CUSHION_S = 0.06;
/** If playback is scheduled further ahead than this, drop to stay near live. */
export const MAX_LEAD_S = 0.4;

export class PlaybackClock {
  private next = 0;

  /**
   * @param now the audio clock (AudioContext.currentTime)
   * @param duration the chunk's length in seconds
   * @returns when to start it, or null if it should be dropped (too far ahead)
   */
  schedule(now: number, duration: number): number | null {
    if (this.next < now) {
      // Underrun (or first chunk): restart with a cushion.
      this.next = now + CUSHION_S;
    } else if (this.next - now > MAX_LEAD_S) {
      return null;
    }
    const start = this.next;
    this.next += duration;
    return start;
  }

  reset() {
    this.next = 0;
  }
}

/** Members whose last frame arrived within `windowMs` of `now` are speaking. */
export function speakingNow(lastFrameAt: Map<string, number>, now: number, windowMs = 300): string[] {
  const out: string[] = [];
  for (const [id, at] of lastFrameAt) if (now - at <= windowMs) out.push(id);
  return out;
}
