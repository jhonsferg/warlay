// The playback side of one remote speaker, as a ring of decoded audio that the
// audio thread reads from in fixed blocks. It runs inside an AudioWorklet, so
// playback no longer depends on the page's main thread being free at the right
// moment (each chunk used to be scheduled from a main-thread callback). Pure
// (no Web Audio types) so it can be unit tested.

import { JitterEstimator, MAX_EXCESS_S } from "./voiceJitter.ts";

interface Chunk {
  channels: Float32Array[];
  /** Frames of this chunk already played. */
  offset: number;
}

export class JitterBuffer {
  private readonly queue: Chunk[] = [];
  private queued = 0;
  private playing = false;
  /** Frames waited while filling up before the first (or the next) play. */
  private waited = 0;
  private readonly estimator = new JitterEstimator();

  private readonly channelCount: number;
  private readonly sampleRate: number;

  constructor(channelCount: number, sampleRate: number) {
    this.channelCount = channelCount;
    this.sampleRate = sampleRate;
  }

  /** Seconds of audio waiting to be played. */
  get bufferedSeconds(): number {
    return this.queued / this.sampleRate;
  }

  /** The cushion currently in use, in seconds. */
  get cushionSeconds(): number {
    return this.estimator.cushion;
  }

  /**
   * A decoded chunk arrived. \`now\` is the audio clock in seconds.
   * Returns false when it was dropped for being too far ahead of playback.
   */
  push(now: number, channels: Float32Array[]): boolean {
    const frames = channels[0]?.length ?? 0;
    if (frames === 0) return true;
    this.estimator.observe(now, frames / this.sampleRate);
    // A backlog is cut so a burst never leaves the speaker lagging behind.
    if (this.playing && this.bufferedSeconds > this.estimator.cushion + MAX_EXCESS_S) return false;
    this.queue.push({ channels, offset: 0 });
    this.queued += frames;
    return true;
  }

  /**
   * Fills \`out\` (one array per channel, one block long) with the next audio,
   * or silence while filling up or after running dry.
   */
  read(out: Float32Array[]): void {
    const block = out[0]?.length ?? 0;
    if (block === 0) return;
    const cushionFrames = Math.round(this.estimator.cushion * this.sampleRate);

    if (!this.playing) {
      if (this.queued > 0) this.waited += block;
      // Starts once the cushion is there, or once waiting that long: a short
      // burst of speech (less than the cushion) must still be heard.
      if (this.queued > 0 && (this.queued >= cushionFrames || this.waited >= cushionFrames)) {
        this.playing = true;
      } else {
        for (const channel of out) channel.fill(0);
        return;
      }
    }

    let written = 0;
    while (written < block && this.queue.length > 0) {
      const chunk = this.queue[0];
      const available = chunk.channels[0].length - chunk.offset;
      const take = Math.min(available, block - written);
      for (let c = 0; c < out.length; c++) {
        // A mono chunk feeds every output channel.
        const source = chunk.channels[Math.min(c, this.channelCount - 1, chunk.channels.length - 1)];
        out[c].set(source.subarray(chunk.offset, chunk.offset + take), written);
      }
      chunk.offset += take;
      written += take;
      this.queued -= take;
      if (chunk.offset >= chunk.channels[0].length) this.queue.shift();
    }

    if (written < block) {
      // Ran dry: silence for the rest, then fill up again before resuming.
      for (const channel of out) channel.fill(0, written);
      this.playing = false;
      this.waited = 0;
    }
  }

  reset() {
    this.queue.length = 0;
    this.queued = 0;
    this.playing = false;
    this.waited = 0;
    this.estimator.reset();
  }
}
