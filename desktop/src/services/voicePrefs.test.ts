import assert from "node:assert/strict";
import { test } from "node:test";
import { audioConstraints, classifyMicError, normalizeNoiseLevel, opusFrameMicros, usesRnnoise } from "./voicePrefs.ts";

test("an unknown or missing noise level falls back to standard", () => {
  assert.equal(normalizeNoiseLevel("strong"), "strong");
  assert.equal(normalizeNoiseLevel("off"), "off");
  assert.equal(normalizeNoiseLevel("standard"), "standard");
  for (const bad of [undefined, null, "", "max", 3, {}]) assert.equal(normalizeNoiseLevel(bad), "standard");
});

test("standard keeps the browser's noise suppression on", () => {
  const c = audioConstraints({ deviceId: "", noise: "standard" });
  assert.equal(c.noiseSuppression, true);
  assert.equal(c.echoCancellation, true);
  assert.equal(c.autoGainControl, true);
});

test("off turns noise suppression off but keeps echo cancellation (speakers!)", () => {
  const c = audioConstraints({ deviceId: "", noise: "off" });
  assert.equal(c.noiseSuppression, false);
  assert.equal(c.echoCancellation, true);
});

test("strong disables the browser's suppressor because RNNoise replaces it", () => {
  const c = audioConstraints({ deviceId: "", noise: "strong" });
  assert.equal(c.noiseSuppression, false);
  assert.equal(c.echoCancellation, true);
  assert.equal(usesRnnoise("strong"), true);
  assert.equal(usesRnnoise("standard"), false);
  assert.equal(usesRnnoise("off"), false);
});

test("a chosen device is requested exactly; the default is not pinned", () => {
  assert.deepEqual(audioConstraints({ deviceId: "mic-1", noise: "standard" }).deviceId, { exact: "mic-1" });
  assert.equal(audioConstraints({ deviceId: "", noise: "standard" }).deviceId, undefined);
});

test("capture is always mono at 48 kHz, what the Opus encoder is configured for", () => {
  const c = audioConstraints({ deviceId: "", noise: "standard" });
  assert.equal(c.channelCount, 1);
  assert.equal(c.sampleRate, 48000);
});

test("a denial after the user consented means Windows itself is blocking the mic", () => {
  const err = { name: "NotAllowedError" };
  assert.equal(classifyMicError(err, true).kind, "blocked-by-os");
  assert.equal(classifyMicError(err, false).kind, "declined");
});

test("other getUserMedia failures are told apart", () => {
  assert.equal(classifyMicError({ name: "NotFoundError" }, true).kind, "no-device");
  assert.equal(classifyMicError({ name: "OverconstrainedError" }, true).kind, "device-unavailable");
  assert.equal(classifyMicError({ name: "NotReadableError" }, true).kind, "busy");
  assert.equal(classifyMicError(new Error("boom"), true).kind, "unknown");
  assert.equal(classifyMicError("weird", true).kind, "unknown");
});

test("every failure carries a message in Spanish for the user", () => {
  for (const name of ["NotAllowedError", "NotFoundError", "OverconstrainedError", "NotReadableError", "Other"]) {
    const { message } = classifyMicError({ name }, true);
    assert.ok(message.length > 10, name);
  }
  assert.match(classifyMicError({ name: "NotAllowedError" }, true).message, /Windows/);
});

test("low-latency voice halves the Opus packet length", () => {
  assert.equal(opusFrameMicros(false), 20000);
  assert.equal(opusFrameMicros(undefined), 20000);
  assert.equal(opusFrameMicros(true), 10000);
});
