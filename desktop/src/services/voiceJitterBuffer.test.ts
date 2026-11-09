import assert from "node:assert/strict";
import { test } from "node:test";
import { JitterBuffer } from "./voiceJitterBuffer.ts";
import { CUSHION_S, MAX_EXCESS_S } from "./voiceJitter.ts";

const RATE = 48000;
const CHUNK = 960; // 20 ms
const BLOCK = 128; // one audio-thread render quantum

const chunkOf = (value: number, frames = CHUNK) => [new Float32Array(frames).fill(value)];
const blockOut = (channels = 1) => Array.from({ length: channels }, () => new Float32Array(BLOCK).fill(-1));

function readBlocks(buffer: JitterBuffer, blocks: number, channels = 1): Float32Array[][] {
  return Array.from({ length: blocks }, () => {
    const out = blockOut(channels);
    buffer.read(out);
    return out;
  });
}

test("it is silent until the cushion has filled", () => {
  const buffer = new JitterBuffer(1, RATE);
  buffer.push(10, chunkOf(0.5));
  const [first] = readBlocks(buffer, 1);
  assert.ok(first[0].every((s) => s === 0));
});

test("it plays what arrived once the cushion is there, in order", () => {
  const buffer = new JitterBuffer(1, RATE);
  for (let i = 0; i < 4; i++) buffer.push(10 + i * 0.02, chunkOf(i + 1));
  // 4 x 20 ms is above the cushion, so playback has started.
  const blocks = readBlocks(buffer, Math.ceil((CHUNK * 2) / BLOCK));
  const samples = blocks.flatMap((b) => [...b[0]]);
  assert.equal(samples[0], 1);
  assert.equal(samples[CHUNK], 2);
});

test("a short burst below the cushion is still played after waiting", () => {
  const buffer = new JitterBuffer(1, RATE);
  buffer.push(10, chunkOf(0.25));
  const blocks = readBlocks(buffer, Math.ceil((CUSHION_S * RATE) / BLOCK) + 2);
  assert.ok(blocks.some((b) => b[0].some((s) => s === 0.25)));
});

test("running dry gives silence, then it fills up again before resuming", () => {
  const buffer = new JitterBuffer(1, RATE);
  for (let i = 0; i < 4; i++) buffer.push(10 + i * 0.02, chunkOf(1));
  readBlocks(buffer, Math.ceil((CHUNK * 4) / BLOCK) + 1); // drain it all
  // One new chunk is below the cushion: it waits instead of stuttering.
  buffer.push(11, chunkOf(1));
  const [out] = readBlocks(buffer, 1);
  assert.ok(out[0].every((s) => s === 0));
});

test("a backlog beyond the cushion is dropped, not queued", () => {
  const buffer = new JitterBuffer(1, RATE);
  for (let i = 0; i < 4; i++) buffer.push(10 + i * 0.02, chunkOf(1));
  readBlocks(buffer, 1); // playing now
  let dropped = 0;
  for (let i = 0; i < 100; i++) if (!buffer.push(10.1, chunkOf(1))) dropped++;
  assert.ok(dropped > 0);
  assert.ok(buffer.bufferedSeconds <= buffer.cushionSeconds + MAX_EXCESS_S + CHUNK / RATE + 1e-9);
});

test("a mono chunk feeds both output channels", () => {
  const buffer = new JitterBuffer(1, RATE);
  for (let i = 0; i < 4; i++) buffer.push(10 + i * 0.02, chunkOf(0.5));
  const [out] = readBlocks(buffer, 1, 2);
  assert.ok(out[0].every((s) => s === 0.5));
  assert.ok(out[1].every((s) => s === 0.5));
});

test("stereo chunks keep their channels apart", () => {
  const buffer = new JitterBuffer(2, RATE);
  for (let i = 0; i < 4; i++)
    buffer.push(10 + i * 0.02, [new Float32Array(CHUNK).fill(0.1), new Float32Array(CHUNK).fill(0.9)]);
  const [out] = readBlocks(buffer, 1, 2);
  assert.ok(out[0].every((s) => s === Math.fround(0.1)));
  assert.ok(out[1].every((s) => s === Math.fround(0.9)));
});

test("reset empties it", () => {
  const buffer = new JitterBuffer(1, RATE);
  for (let i = 0; i < 4; i++) buffer.push(10 + i * 0.02, chunkOf(1));
  buffer.reset();
  assert.equal(buffer.bufferedSeconds, 0);
  const [out] = readBlocks(buffer, 1);
  assert.ok(out[0].every((s) => s === 0));
});
