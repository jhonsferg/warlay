// Inbound voice and desktop-audio frames arrive from the desktop shell as one
// binary message per batch (see `runtime/voice.rs`): for every frame, the
// sender's client id (16 bytes), the payload length (u16, little endian) and
// the payload. This reads it back; pure, so it can be unit tested.

export interface InboundFrame {
  /** The sending member's client id (a UUID). */
  from: string;
  data: Uint8Array;
}

const ID_BYTES = 16;
const HEADER_BYTES = ID_BYTES + 2;

function uuidOf(bytes: Uint8Array, at: number): string {
  let hex = "";
  for (let i = 0; i < ID_BYTES; i++) hex += bytes[at + i].toString(16).padStart(2, "0");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** Reads a batch. A truncated tail (should never happen) is ignored. */
export function decodeFrameBatch(message: ArrayBuffer | ArrayLike<number>): InboundFrame[] {
  const bytes =
    message instanceof ArrayBuffer ? new Uint8Array(message) : Uint8Array.from(message as ArrayLike<number>);
  const frames: InboundFrame[] = [];
  let at = 0;
  while (at + HEADER_BYTES <= bytes.length) {
    const length = bytes[at + ID_BYTES] | (bytes[at + ID_BYTES + 1] << 8);
    const start = at + HEADER_BYTES;
    if (start + length > bytes.length) break;
    // `slice` copies: the decoder keeps the frame after the message is gone.
    frames.push({ from: uuidOf(bytes, at), data: bytes.slice(start, start + length) });
    at = start + length;
  }
  return frames;
}
