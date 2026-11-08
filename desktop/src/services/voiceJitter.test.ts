import assert from "node:assert/strict";
import { test } from "node:test";
import { CUSHION_S, MAX_LEAD_S, PlaybackClock, speakingNow } from "./voiceJitter.ts";

test("the first chunk starts after a small cushion", () => {
  const clock = new PlaybackClock();
  assert.equal(clock.schedule(10, 0.02), 10 + CUSHION_S);
});

test("consecutive chunks are scheduled back to back", () => {
  const clock = new PlaybackClock();
  const first = clock.schedule(10, 0.02)!;
  const second = clock.schedule(10.001, 0.02)!;
  const third = clock.schedule(10.002, 0.02)!;
  assert.ok(Math.abs(second - (first + 0.02)) < 1e-9);
  assert.ok(Math.abs(third - (second + 0.02)) < 1e-9);
});

test("an underrun restarts with a fresh cushion instead of playing in the past", () => {
  const clock = new PlaybackClock();
  clock.schedule(10, 0.02);
  // The speaker paused for a second: the next chunk must not be in the past.
  assert.equal(clock.schedule(11, 0.02), 11 + CUSHION_S);
});

test("a burst beyond the maximum lead is dropped to stay near live", () => {
  const clock = new PlaybackClock();
  let dropped = 0;
  for (let i = 0; i < 100; i++) if (clock.schedule(10, 0.02) === null) dropped++;
  assert.ok(dropped > 0);
  // What was kept never runs further ahead than the limit plus one chunk.
  const next = clock.schedule(10, 0.02);
  assert.equal(next, null);
  assert.ok(MAX_LEAD_S > CUSHION_S);
});

test("reset forgets the schedule", () => {
  const clock = new PlaybackClock();
  clock.schedule(10, 0.02);
  clock.reset();
  assert.equal(clock.schedule(10, 0.02), 10 + CUSHION_S);
});

test("speakingNow lists only members heard within the window", () => {
  const last = new Map([
    ["a", 1000],
    ["b", 600],
    ["c", 990],
  ]);
  assert.deepEqual(speakingNow(last, 1000).sort(), ["a", "c"]);
  assert.deepEqual(speakingNow(last, 1000, 500).sort(), ["a", "b", "c"]);
  assert.deepEqual(speakingNow(new Map(), 1000), []);
});
