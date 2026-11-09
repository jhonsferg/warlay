import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeFrameBatch } from "./frameBatch.ts";

function idBytes(hex: string): number[] {
  return hex.match(/../g)!.map((h) => parseInt(h, 16));
}

const A = "0123456789abcdef0123456789abcdef";
const A_UUID = "01234567-89ab-cdef-0123-456789abcdef";
const B = "ffeeddccbbaa99887766554433221100";
const B_UUID = "ffeeddcc-bbaa-9988-7766-554433221100";

test("reads frames in order with their sender and payload", () => {
  const bytes = new Uint8Array([...idBytes(A), 3, 0, 1, 2, 3, ...idBytes(B), 0, 0, ...idBytes(A), 1, 0, 255]);
  const frames = decodeFrameBatch(bytes.buffer);
  assert.equal(frames.length, 3);
  assert.deepEqual(frames[0], { from: A_UUID, data: new Uint8Array([1, 2, 3]) });
  assert.deepEqual(frames[1], { from: B_UUID, data: new Uint8Array([]) });
  assert.deepEqual(frames[2], { from: A_UUID, data: new Uint8Array([255]) });
});

test("an empty batch has no frames", () => {
  assert.deepEqual(decodeFrameBatch(new ArrayBuffer(0)), []);
});

test("a length above 255 uses both length bytes", () => {
  const payload = new Array(300).fill(7);
  const bytes = new Uint8Array([...idBytes(A), 300 & 255, 300 >> 8, ...payload]);
  const [frame] = decodeFrameBatch(bytes.buffer);
  assert.equal(frame.data.length, 300);
});

test("a truncated tail is ignored instead of throwing", () => {
  const bytes = new Uint8Array([...idBytes(A), 1, 0, 9, ...idBytes(B), 5, 0, 1]);
  const frames = decodeFrameBatch(bytes.buffer);
  assert.equal(frames.length, 1);
  assert.equal(frames[0].from, A_UUID);
});

test("accepts a plain number array too", () => {
  const frames = decodeFrameBatch([...idBytes(A), 1, 0, 42]);
  assert.deepEqual(frames[0].data, new Uint8Array([42]));
});

test("the frame is a copy, not a view of the message", () => {
  const bytes = new Uint8Array([...idBytes(A), 1, 0, 42]);
  const [frame] = decodeFrameBatch(bytes.buffer);
  bytes[18] = 0;
  assert.equal(frame.data[0], 42);
});
