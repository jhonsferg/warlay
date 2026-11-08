import assert from "node:assert/strict";
import { test } from "node:test";
import { chooseCaptureTrack, type CaptureFilter } from "./captureChain.ts";

const rawTrack = { id: "raw" };
const cleanTrack = { id: "clean" };

function filterThatWorks(): { create: () => Promise<CaptureFilter<typeof cleanTrack>>; disposed: () => boolean } {
  let disposed = false;
  return {
    create: async () => ({
      track: cleanTrack,
      dispose: () => {
        disposed = true;
      },
    }),
    disposed: () => disposed,
  };
}

test("off and standard use the microphone track untouched and never build a filter", async () => {
  for (const level of ["off", "standard"] as const) {
    let built = 0;
    const result = await chooseCaptureTrack(level, rawTrack, async () => {
      built++;
      throw new Error("must not be called");
    });
    assert.equal(result.track, rawTrack);
    assert.equal(result.filtered, false);
    assert.equal(result.fallbackReason, undefined);
    assert.equal(built, 0, level);
  }
});

test("strong swaps in the filtered track", async () => {
  const f = filterThatWorks();
  const result = await chooseCaptureTrack("strong", rawTrack, f.create);
  assert.equal(result.track, cleanTrack);
  assert.equal(result.filtered, true);
  assert.equal(f.disposed(), false);
  result.dispose();
  assert.equal(f.disposed(), true);
});

test("if the filter cannot start, voice keeps working on the raw track and says why", async () => {
  const result = await chooseCaptureTrack("strong", rawTrack, async () => {
    throw new Error("wasm blocked by CSP");
  });
  assert.equal(result.track, rawTrack);
  assert.equal(result.filtered, false);
  assert.match(result.fallbackReason ?? "", /wasm blocked by CSP/);
  // Disposing a fallback is harmless.
  assert.doesNotThrow(() => result.dispose());
});

test("a non-Error rejection still produces a readable reason", async () => {
  const result = await chooseCaptureTrack("strong", rawTrack, async () => {
    throw "nope";
  });
  assert.equal(result.filtered, false);
  assert.equal(result.fallbackReason, "nope");
});
