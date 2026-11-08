/**
 * Who is in voice and whether their mic is muted, like the mic icons of Meet
 * and Teams. Each member announces it with a tiny control packet sent through
 * the same unreliable voice channel as the audio (so it needs no new network
 * path), on every change and as a heartbeat, because a datagram can be lost
 * and someone may join late. Pure (no Vue, no Tauri) so it can be unit tested.
 */

/** First byte of a control packet. As an Opus TOC byte it would mean
 * "stereo, code 3", which the mono voice encoder never produces, so a control
 * packet can never be mistaken for audio. */
const MARKER = 0xff;
const KIND_MUTE = 0xfe;

/** How often a member in voice repeats its state. */
export const HEARTBEAT_MS = 2000;
/** A member not heard from for this long (three missed heartbeats) is gone. */
export const PRESENCE_TTL_MS = 7000;

export type VoiceState = "muted" | "live";

/** What a control packet can announce: a state, or that the member left voice. */
export type VoiceAnnouncement = VoiceState | "left";

const CODES: Record<VoiceAnnouncement, number> = { live: 0, muted: 1, left: 2 };

export function encodeStateFrame(state: VoiceAnnouncement): Uint8Array {
  return Uint8Array.of(MARKER, KIND_MUTE, CODES[state]);
}

/** What a control packet announces, or null for audio and anything else. */
export function decodeStateFrame(frame: Uint8Array): VoiceAnnouncement | null {
  if (frame.length !== 3 || frame[0] !== MARKER || frame[1] !== KIND_MUTE) return null;
  const found = (Object.entries(CODES) as [VoiceAnnouncement, number][]).find(([, code]) => code === frame[2]);
  return found ? found[0] : null;
}

/** Last announced state of each member, forgotten after `PRESENCE_TTL_MS`. */
export class VoicePresence {
  private readonly seen = new Map<string, { state: VoiceState; at: number }>();

  update(id: string, state: VoiceState, now: number) {
    this.seen.set(id, { state, at: now });
  }

  /** `null`: not in voice (never announced, or silent for too long). */
  stateOf(id: string, now: number): VoiceState | null {
    const entry = this.seen.get(id);
    if (!entry || now - entry.at > PRESENCE_TTL_MS) return null;
    return entry.state;
  }

  /** Everyone currently in voice with their state. */
  snapshot(now: number): Record<string, VoiceState> {
    const out: Record<string, VoiceState> = {};
    for (const [id, { state, at }] of this.seen) if (now - at <= PRESENCE_TTL_MS) out[id] = state;
    return out;
  }

  forget(id: string) {
    this.seen.delete(id);
  }

  clear() {
    this.seen.clear();
  }
}
