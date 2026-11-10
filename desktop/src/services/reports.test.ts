import assert from "node:assert/strict";
import { test } from "node:test";
import { canSendReport, isReportReason, REPORT_DETAILS_MAX, REPORT_REASONS } from "./reports.ts";

test("the reasons offered are the ones the server accepts, each with a label", () => {
  assert.deepEqual(REPORT_REASONS.map((r) => r.id).sort(), [
    "cheating",
    "harassment",
    "impersonation",
    "inappropriate",
    "other",
    "spam",
  ]);
  for (const reason of REPORT_REASONS) assert.ok(reason.label.length > 3);
});

test("only a known reason is a reason", () => {
  assert.equal(isReportReason("spam"), true);
  assert.equal(isReportReason(""), false);
  assert.equal(isReportReason("Spam"), false);
  assert.equal(isReportReason("something else"), false);
});

test("a report needs a reason and an explanation within the limit", () => {
  assert.equal(canSendReport("harassment", ""), true);
  assert.equal(canSendReport("harassment", "x".repeat(REPORT_DETAILS_MAX)), true);
  assert.equal(canSendReport("harassment", "x".repeat(REPORT_DETAILS_MAX + 1)), false);
  assert.equal(canSendReport("", "algo"), false);
  assert.equal(
    canSendReport("harassment", `   ${"x".repeat(REPORT_DETAILS_MAX)}   `),
    true,
    "spaces around do not count",
  );
});
