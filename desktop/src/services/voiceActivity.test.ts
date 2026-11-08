import assert from "node:assert/strict";
import { test } from "node:test";
import { HANGOVER_FRAMES, meterLevel, OPEN_LEVEL, rms, smoothLevel, VoiceGate } from "./voiceActivity.ts";

const LOUD = OPEN_LEVEL * 10;

test("rms of silence is 0 and of a full-scale square wave is 1", () => {
  assert.equal(rms([]), 0);
  assert.equal(rms(new Float32Array(960)), 0);
  assert.equal(rms([1, -1, 1, -1]), 1);
  assert.ok(Math.abs(rms([0.5, -0.5]) - 0.5) < 1e-9);
});

test("a silent microphone sends nothing", () => {
  const gate = new VoiceGate();
  for (let i = 0; i < 100; i++) assert.equal(gate.pass(0.001), false);
});

test("speech opens the gate at once", () => {
  const gate = new VoiceGate();
  assert.equal(gate.pass(LOUD), true);
});

test("the gate stays open for the hangover and then closes", () => {
  const gate = new VoiceGate();
  gate.pass(LOUD);
  for (let i = 0; i < HANGOVER_FRAMES; i++) assert.equal(gate.pass(0), true, `hangover frame ${i}`);
  assert.equal(gate.pass(0), false);
});

test("speech during the hangover renews it", () => {
  const gate = new VoiceGate();
  gate.pass(LOUD);
  for (let i = 0; i < HANGOVER_FRAMES - 1; i++) gate.pass(0);
  gate.pass(LOUD);
  for (let i = 0; i < HANGOVER_FRAMES; i++) assert.equal(gate.pass(0), true);
  assert.equal(gate.pass(0), false);
});

test("reset drops the hangover", () => {
  const gate = new VoiceGate();
  gate.pass(LOUD);
  gate.reset();
  assert.equal(gate.pass(0), false);
});

test("the meter fills at 0.2 rms and never leaves 0..1", () => {
  assert.equal(meterLevel(0), 0);
  assert.ok(Math.abs(meterLevel(0.1) - 0.5) < 1e-9);
  assert.equal(meterLevel(0.5), 1);
  assert.equal(meterLevel(-1), 0);
});

test("the meter rises at once and falls gradually", () => {
  assert.equal(smoothLevel(0.2, 0.9), 0.9);
  const fell = smoothLevel(1, 0);
  assert.ok(fell > 0 && fell < 1);
  let level = 1;
  for (let i = 0; i < 20; i++) level = smoothLevel(level, 0);
  assert.ok(level < 0.01, "it ends at silence");
});
