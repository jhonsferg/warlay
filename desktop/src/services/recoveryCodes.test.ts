import assert from "node:assert/strict";
import { test } from "node:test";
import { codesText, looksLikeCode } from "./recoveryCodes.ts";

test("the shareable text lists every code, numbered, under the account name", () => {
  const text = codesText("thrall", ["ABCDE-12345", "FGHJK-67890"]);
  assert.match(text, /recovery codes for @thrall/);
  assert.match(text, / 1\. ABCDE-12345/);
  assert.match(text, / 2\. FGHJK-67890/);
});

test("a code is recognised however it is typed", () => {
  for (const typed of ["ABCDE-12345", "abcde12345", " abcde 12345 ", "OOOOO-IIIII", "lllll-00000"]) {
    assert.equal(looksLikeCode(typed), true, typed);
  }
});

test("things that cannot be a code are rejected before asking the server", () => {
  for (const bad of ["", "ABCDE", "ABCDE-1234", "ABCDE-123456", "ABCDE-1234U", "ABCDE-1234!"]) {
    assert.equal(looksLikeCode(bad), false, bad);
  }
});
