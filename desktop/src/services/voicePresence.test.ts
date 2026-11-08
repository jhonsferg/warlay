import assert from "node:assert/strict";
import { test } from "node:test";
import { decodeStateFrame, encodeStateFrame, PRESENCE_TTL_MS, VoicePresence } from "./voicePresence.ts";

test("a state packet round-trips and is tiny", () => {
  for (const state of ["muted", "live", "left"] as const) {
    const frame = encodeStateFrame(state);
    assert.equal(frame.length, 3);
    assert.equal(decodeStateFrame(frame), state);
  }
});

test("audio and malformed packets are never taken for a state", () => {
  assert.equal(decodeStateFrame(new Uint8Array(0)), null);
  assert.equal(decodeStateFrame(Uint8Array.of(0xfc, 0xff, 0xfe)), null, "a plain opus packet");
  assert.equal(decodeStateFrame(Uint8Array.of(0xff, 0xfe)), null, "too short");
  assert.equal(decodeStateFrame(Uint8Array.of(0xff, 0xfe, 1, 0)), null, "too long");
  assert.equal(decodeStateFrame(Uint8Array.of(0xff, 0xfe, 9)), null, "unknown state");
  assert.equal(decodeStateFrame(Uint8Array.of(0xff, 0xfd, 1)), null, "unknown kind");
});

test("an unknown member is not in voice", () => {
  assert.equal(new VoicePresence().stateOf("a", 0), null);
});

test("the last announced state wins", () => {
  const p = new VoicePresence();
  p.update("a", "live", 0);
  p.update("a", "muted", 10);
  assert.equal(p.stateOf("a", 20), "muted");
  p.update("a", "live", 30);
  assert.equal(p.stateOf("a", 40), "live");
});

test("a member that stops announcing leaves voice after the ttl", () => {
  const p = new VoicePresence();
  p.update("a", "muted", 1000);
  assert.equal(p.stateOf("a", 1000 + PRESENCE_TTL_MS), "muted");
  assert.equal(p.stateOf("a", 1000 + PRESENCE_TTL_MS + 1), null);
  assert.deepEqual(p.snapshot(1000 + PRESENCE_TTL_MS + 1), {});
});

test("a heartbeat keeps a member in voice", () => {
  const p = new VoicePresence();
  p.update("a", "live", 0);
  p.update("a", "live", 6000);
  assert.equal(p.stateOf("a", 12000), "live");
});

test("snapshot lists everyone in voice; forget and clear drop them", () => {
  const p = new VoicePresence();
  p.update("a", "live", 0);
  p.update("b", "muted", 0);
  assert.deepEqual(p.snapshot(1), { a: "live", b: "muted" });
  p.forget("a");
  assert.deepEqual(p.snapshot(1), { b: "muted" });
  p.clear();
  assert.deepEqual(p.snapshot(1), {});
});
