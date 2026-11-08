import { audioConstraints, type MicPrefs } from "./voicePrefs";
import { PlaybackClock } from "./voiceJitter";

/**
 * In-room voice: the microphone is captured and Opus-encoded here in the
 * webview (browser echo cancellation / noise suppression / AGC included) and
 * played back here; Rust only carries the opaque frames over the P2P mesh.
 */

const SAMPLE_RATE = 48000;
const CHANNELS = 1;
const BITRATE = 32000;

// WebCodecs / insertable-streams types the TS DOM lib may not ship yet.
/* eslint-disable @typescript-eslint/no-explicit-any */
type AnyCtor = new (...args: any[]) => any;
const g = globalThis as any;

export function voiceSupported(): boolean {
  return (
    typeof g.AudioEncoder === "function" &&
    typeof g.AudioDecoder === "function" &&
    typeof g.MediaStreamTrackProcessor === "function" &&
    !!navigator.mediaDevices?.getUserMedia
  );
}

export interface VoiceTransport {
  send(frame: Uint8Array): void;
}

export class VoiceEngine {
  private stream: MediaStream | null = null;
  private encoder: any = null;
  private reader: any = null;
  private ctx: AudioContext | null = null;
  private readonly peers = new Map<string, { decoder: any; clock: PlaybackClock; ts: number }>();
  private muted = true;
  private running = false;

  constructor(private readonly transport: VoiceTransport) {}

  isRunning() {
    return this.running;
  }

  /** Opens the microphone and starts encoding. Starts muted-state per `muted`. */
  async start(prefs: MicPrefs = { deviceId: "", noise: "standard" }): Promise<void> {
    if (this.running) return;
    if (!voiceSupported()) throw new Error("Este entorno no soporta WebCodecs/Opus para voz.");

    this.stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints(prefs) });
    this.ctx = new AudioContext({ sampleRate: SAMPLE_RATE, latencyHint: "interactive" });

    const Encoder = g.AudioEncoder as AnyCtor;
    this.encoder = new Encoder({
      output: (chunk: any) => {
        if (this.muted) return;
        const frame = new Uint8Array(chunk.byteLength);
        chunk.copyTo(frame);
        this.transport.send(frame);
      },
      error: (e: unknown) => console.error("voice encoder error", e),
    });
    this.encoder.configure({
      codec: "opus",
      sampleRate: SAMPLE_RATE,
      numberOfChannels: CHANNELS,
      bitrate: BITRATE,
      opus: { frameDuration: 20000, application: "voip", signal: "voice" },
    });

    const [track] = this.stream.getAudioTracks();
    const Processor = g.MediaStreamTrackProcessor as AnyCtor;
    this.reader = new Processor({ track }).readable.getReader();
    this.running = true;
    void this.pump();
  }

  private async pump() {
    try {
      while (this.running) {
        const { value, done } = await this.reader.read();
        if (done || !value) break;
        // While muted nothing is encoded or sent at all.
        if (!this.muted && this.encoder?.state === "configured") this.encoder.encode(value);
        value.close();
      }
    } catch (e) {
      if (this.running) console.error("voice capture stopped", e);
    }
  }

  setMuted(muted: boolean) {
    this.muted = muted;
    // Also tell the OS the mic is not in use (the browser's recording
    // indicator goes off while muted, like Meet).
    this.stream?.getAudioTracks().forEach((t) => (t.enabled = !muted));
  }

  isMuted() {
    return this.muted;
  }

  /** An encoded frame from another member. */
  push(from: string, frame: Uint8Array) {
    if (!this.ctx) return; // not in voice: ignore
    let peer = this.peers.get(from);
    if (!peer) {
      const Decoder = g.AudioDecoder as AnyCtor;
      const decoder = new Decoder({
        output: (data: any) => this.play(from, data),
        error: (e: unknown) => console.error("voice decoder error", e),
      });
      decoder.configure({ codec: "opus", sampleRate: SAMPLE_RATE, numberOfChannels: CHANNELS });
      peer = { decoder, clock: new PlaybackClock(), ts: 0 };
      this.peers.set(from, peer);
    }
    if (peer.decoder.state !== "configured") return;
    const Chunk = g.EncodedAudioChunk as AnyCtor;
    peer.decoder.decode(new Chunk({ type: "key", timestamp: peer.ts, data: frame }));
    peer.ts += 20000;
  }

  private play(from: string, data: any) {
    const ctx = this.ctx;
    const peer = this.peers.get(from);
    if (!ctx || !peer) {
      data.close();
      return;
    }
    const samples = new Float32Array(data.numberOfFrames);
    data.copyTo(samples, { planeIndex: 0, format: "f32-planar" });
    const duration = data.numberOfFrames / data.sampleRate;
    data.close();
    const start = peer.clock.schedule(ctx.currentTime, duration);
    if (start === null) return;
    const buffer = ctx.createBuffer(CHANNELS, samples.length, SAMPLE_RATE);
    buffer.copyToChannel(samples, 0);
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(ctx.destination);
    src.start(start);
  }

  forget(from: string) {
    this.peers.get(from)?.decoder.close();
    this.peers.delete(from);
  }

  async stop() {
    this.running = false;
    try {
      await this.reader?.cancel();
    } catch {
      /* already closed */
    }
    this.reader = null;
    try {
      this.encoder?.close();
    } catch {
      /* already closed */
    }
    this.encoder = null;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    for (const id of [...this.peers.keys()]) this.forget(id);
    await this.ctx?.close().catch(() => {});
    this.ctx = null;
    this.muted = true;
  }
}
