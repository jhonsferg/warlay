import { chooseCaptureTrack, type CaptureChoice } from "./captureChain";
import { createRnnoiseFilter } from "./noiseSuppression";
import { audioConstraints, opusFrameMicros, type MicPrefs } from "./voicePrefs";
import { rms, VoiceGate } from "./voiceActivity";
import { PlaybackClock } from "./voiceJitter";
import playbackWorkletUrl from "./voicePlayback.worklet.ts?worker&url";

/** The processor registered by `voicePlayback.worklet.ts`. */
const PLAYBACK_PROCESSOR = "warlay-playback";

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

/** Level (0..1) of one captured frame. If it cannot be read the frame counts
 * as loud: better to send silence than to cut someone's voice. */
function levelOf(data: any): number {
  try {
    const samples = new Float32Array(data.numberOfFrames);
    data.copyTo(samples, { planeIndex: 0, format: "f32-planar" });
    return rms(samples);
  } catch {
    return 1;
  }
}

export interface VoiceTransport {
  send(frame: Uint8Array): void;
  /** Level (RMS, 0..1) of a frame being sent (`from` null: this microphone)
   * or just decoded from a member. Drives the speaking rings. */
  onLevel?(from: string | null, level: number): void;
}

interface Peer {
  decoder: any;
  clock: PlaybackClock;
  node: AudioWorkletNode | null;
  ts: number;
}

export class VoiceEngine {
  private stream: MediaStream | null = null;
  private encoder: any = null;
  private reader: any = null;
  private ctx: AudioContext | null = null;
  private capture: CaptureChoice<MediaStreamTrack> | null = null;
  /** Why the strong noise filter is not active, when it was asked for. */
  noiseFallbackReason: string | undefined;
  /** `node` plays through an AudioWorklet's jitter buffer; without one (the worklet could not load) the
   * chunks are scheduled one by one with `clock`. */
  private readonly peers = new Map<string, Peer>();
  /** Shared desktop audio of each member: stereo, independent of their voice. */
  private readonly desktopPeers = new Map<string, Peer>();
  /** Whether the playback worklet is loaded in the current AudioContext. */
  private playbackWorklet = false;
  private muted = true;
  private running = false;
  private readonly gate = new VoiceGate();

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
    this.playbackWorklet = await this.ctx.audioWorklet.addModule(playbackWorkletUrl).then(
      () => true,
      (error: unknown) => {
        console.warn("playback worklet unavailable, scheduling chunks from the page instead:", error);
        return false;
      },
    );

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
      opus: { frameDuration: opusFrameMicros(prefs.lowLatency), application: "voip", signal: "voice" },
    });

    const [rawTrack] = this.stream.getAudioTracks();
    const stream = this.stream;
    const ctx = this.ctx;
    this.capture = await chooseCaptureTrack(prefs.noise, rawTrack, () => createRnnoiseFilter(ctx, stream));
    this.noiseFallbackReason = this.capture.fallbackReason;
    if (prefs.noise === "strong" && !this.capture.filtered) {
      // The strong filter could not start. The capture was opened with the
      // browser's suppressor OFF (RNNoise was meant to replace it), so
      // reopen it with the standard one instead of sending the raw noise.
      console.warn("noise filter unavailable, using the standard suppressor:", this.capture.fallbackReason);
      this.stream.getTracks().forEach((t) => t.stop());
      this.stream = await navigator.mediaDevices.getUserMedia({
        audio: audioConstraints({ ...prefs, noise: "standard" }),
      });
      this.capture = { track: this.stream.getAudioTracks()[0], filtered: false, dispose() {} };
    }
    const track = this.capture.track;
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
        // While muted nothing is encoded or sent at all; unmuted but silent,
        // nothing is sent either, so the others only see who is really talking.
        if (!this.muted && this.encoder?.state === "configured") {
          const level = levelOf(value);
          if (this.gate.pass(level)) {
            this.transport.onLevel?.(null, level);
            this.encoder.encode(value);
          }
        }
        value.close();
      }
    } catch (e) {
      if (this.running) console.error("voice capture stopped", e);
    }
  }

  setMuted(muted: boolean) {
    this.muted = muted;
    this.gate.reset();
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
      peer = { decoder, clock: new PlaybackClock(), node: this.playbackNode(1), ts: 0 };
      this.peers.set(from, peer);
    }
    if (peer.decoder.state !== "configured") return;
    const Chunk = g.EncodedAudioChunk as AnyCtor;
    peer.decoder.decode(new Chunk({ type: "key", timestamp: peer.ts, data: frame }));
    peer.ts += 20000;
  }

  /** An encoded frame of another member's shared desktop audio (stereo Opus). */
  pushDesktop(from: string, frame: Uint8Array) {
    if (!this.ctx) return; // not in voice: ignore
    let peer = this.desktopPeers.get(from);
    if (!peer) {
      const Decoder = g.AudioDecoder as AnyCtor;
      const decoder = new Decoder({
        output: (data: any) => this.playDesktop(from, data),
        error: (e: unknown) => console.error("desktop audio decoder error", e),
      });
      decoder.configure({ codec: "opus", sampleRate: SAMPLE_RATE, numberOfChannels: 2 });
      peer = { decoder, clock: new PlaybackClock(), node: this.playbackNode(2), ts: 0 };
      this.desktopPeers.set(from, peer);
    }
    if (peer.decoder.state !== "configured") return;
    const Chunk = g.EncodedAudioChunk as AnyCtor;
    peer.decoder.decode(new Chunk({ type: "key", timestamp: peer.ts, data: frame }));
    peer.ts += 20000;
  }

  /** A speaker's playback node: it keeps its own jitter buffer on the audio thread. */
  private playbackNode(channels: number): AudioWorkletNode | null {
    const ctx = this.ctx;
    if (!ctx || !this.playbackWorklet) return null;
    try {
      const node = new AudioWorkletNode(ctx, PLAYBACK_PROCESSOR, {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [channels],
        processorOptions: { channels },
      });
      node.connect(ctx.destination);
      return node;
    } catch (error) {
      console.warn("could not create a playback node:", error);
      return null;
    }
  }

  private playDesktop(from: string, data: any) {
    const ctx = this.ctx;
    const peer = this.desktopPeers.get(from);
    if (!ctx || !peer) {
      data.close();
      return;
    }
    const frames: number = data.numberOfFrames;
    const left = new Float32Array(frames);
    const right = new Float32Array(frames);
    data.copyTo(left, { planeIndex: 0, format: "f32-planar" });
    data.copyTo(right, { planeIndex: Math.min(1, data.numberOfChannels - 1), format: "f32-planar" });
    const duration = frames / data.sampleRate;
    data.close();
    if (peer.node) {
      peer.node.port.postMessage([left, right], [left.buffer, right.buffer]);
      return;
    }
    const start = peer.clock.schedule(ctx.currentTime, duration);
    if (start === null) return;
    const buffer = ctx.createBuffer(2, frames, SAMPLE_RATE);
    buffer.copyToChannel(left, 0);
    buffer.copyToChannel(right, 1);
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(ctx.destination);
    src.start(start);
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
    this.transport.onLevel?.(from, rms(samples));
    if (peer.node) {
      peer.node.port.postMessage([samples], [samples.buffer]);
      return;
    }
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
    const peer = this.peers.get(from);
    peer?.decoder.close();
    peer?.node?.disconnect();
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
    this.capture?.dispose();
    this.capture = null;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    for (const id of [...this.peers.keys()]) this.forget(id);
    for (const peer of this.desktopPeers.values()) {
      peer.decoder.close();
      peer.node?.disconnect();
    }
    this.desktopPeers.clear();
    await this.ctx?.close().catch(() => {});
    this.ctx = null;
    this.muted = true;
  }
}
