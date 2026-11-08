import { loadRnnoise, RnnoiseWorkletNode } from "@sapphi-red/web-noise-suppressor";
import rnnoiseWasmUrl from "@sapphi-red/web-noise-suppressor/rnnoise.wasm?url";
import rnnoiseSimdWasmUrl from "@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm?url";
import rnnoiseWorkletUrl from "@sapphi-red/web-noise-suppressor/rnnoiseWorklet.js?url";
import type { CaptureFilter } from "./captureChain";

/**
 * RNNoise (a small recurrent network trained to separate voice from noise)
 * running in an AudioWorklet: microphone -> RNNoise -> a new MediaStream
 * track that feeds the encoder. Needs the WebAssembly the app's CSP allows
 * with `'wasm-unsafe-eval'` (see `tauri.conf.json`).
 *
 * It removes steady and impulsive noise (fans, keyboard, mouse); it does NOT
 * remove another person talking nearby -- that takes speaker-isolation models.
 */
let wasmBinary: Promise<ArrayBuffer> | null = null;
const loadedContexts = new WeakSet<BaseAudioContext>();

/**
 * Without the CSP's `'wasm-unsafe-eval'` the worklet does NOT throw: it is
 * created normally and then emits digital silence, which would mute the
 * microphone with no error to fall back on. The page's CSP applies to the
 * worklet too, so compiling a tiny module here (the same permission) turns
 * that silent failure into an error the capture chain can react to.
 */
export function assertWasmAllowed(): void {
  try {
    new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]));
  } catch (error) {
    throw new Error(`WebAssembly bloqueado por la política de seguridad: ${String(error)}`);
  }
}

export async function createRnnoiseFilter(
  ctx: AudioContext,
  source: MediaStream,
): Promise<CaptureFilter<MediaStreamTrack>> {
  assertWasmAllowed();
  wasmBinary ??= loadRnnoise({ url: rnnoiseWasmUrl, simdUrl: rnnoiseSimdWasmUrl });
  const binary = await wasmBinary.catch((error) => {
    wasmBinary = null; // let the next attempt retry the download
    throw error;
  });
  if (!loadedContexts.has(ctx)) {
    await ctx.audioWorklet.addModule(rnnoiseWorkletUrl);
    loadedContexts.add(ctx);
  }

  const input = ctx.createMediaStreamSource(source);
  const node = new RnnoiseWorkletNode(ctx, { wasmBinary: binary, maxChannels: 1 });
  const output = ctx.createMediaStreamDestination();
  input.connect(node);
  node.connect(output);

  return {
    track: output.stream.getAudioTracks()[0],
    dispose() {
      try {
        node.destroy?.();
      } catch {
        /* already gone */
      }
      input.disconnect();
      node.disconnect();
    },
  };
}
