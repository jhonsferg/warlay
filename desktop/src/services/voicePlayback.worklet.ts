// Runs on the audio thread (an AudioWorklet): plays one remote speaker from a
// jitter buffer. The page decodes each frame and posts the samples here; the
// buffer smooths the network's uneven pace, so playback no longer depends on
// the main thread being free at the right moment.
import { JitterBuffer } from "./voiceJitterBuffer.ts";

// The worklet scope's globals, which the DOM typings do not describe.
declare const sampleRate: number;
declare const currentTime: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: new (options: unknown) => AudioWorkletProcessor): void;

export const PLAYBACK_PROCESSOR = "warlay-playback";

class PlaybackProcessor extends AudioWorkletProcessor {
  private readonly buffer: JitterBuffer;

  constructor(options: unknown) {
    super();
    const channels = (options as { processorOptions?: { channels?: number } }).processorOptions?.channels ?? 1;
    this.buffer = new JitterBuffer(channels, sampleRate);
    this.port.onmessage = (event: MessageEvent<Float32Array[]>) => {
      this.buffer.push(currentTime, event.data);
    };
  }

  process(_inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    this.buffer.read(outputs[0]);
    return true;
  }
}

registerProcessor(PLAYBACK_PROCESSOR, PlaybackProcessor);
