import assert from "node:assert/strict";
import { test } from "node:test";
import { googleFailureMessage, isGoogleCancel } from "./googleSignIn.ts";

test("every code the shell and the website can send has its own sentence", () => {
  for (const code of [
    "cancelled",
    "timeout",
    "access_denied",
    "email_not_verified",
    "account_conflict",
    "provider_refused",
  ]) {
    assert.notEqual(googleFailureMessage(code), googleFailureMessage("something-new"), code);
  }
});

test("a code with detail after it, or wrapped in an Error, is still recognised", () => {
  assert.equal(googleFailureMessage("open_failed: access denied"), googleFailureMessage("open_failed"));
  assert.equal(googleFailureMessage(new Error("timeout")), googleFailureMessage("timeout"));
});

test("anything unknown gets the generic sentence, never a raw code", () => {
  assert.match(googleFailureMessage("zzz"), /Inténtalo de nuevo/);
  assert.match(googleFailureMessage(undefined), /Inténtalo de nuevo/);
});

test("giving up is told apart from failing", () => {
  assert.equal(isGoogleCancel(new Error(googleFailureMessage("cancelled"))), true);
  assert.equal(isGoogleCancel(new Error(googleFailureMessage("access_denied"))), true);
  assert.equal(isGoogleCancel(new Error(googleFailureMessage("timeout"))), false);
});
