import assert from "node:assert/strict";
import { test } from "node:test";
import { copyrightLine, formatDate } from "./about.ts";

test("a known timestamp reads as a long date in Spanish", () => {
  // Midday UTC, so every timezone lands on the same calendar day.
  const text = formatDate(Date.UTC(2026, 10, 9, 12) / 1000);
  assert.match(text, /9/);
  assert.match(text, /noviembre/i);
  assert.match(text, /2026/);
});

test("a missing or invalid timestamp shows a dash", () => {
  for (const bad of [null, undefined, 0, -5, Number.NaN]) assert.equal(formatDate(bad), "—");
});

test("the copyright line carries the year and the rights notice", () => {
  assert.equal(copyrightLine(2026), "© 2026 Warlay. Todos los derechos reservados.");
  assert.equal(copyrightLine(2027, "Acme"), "© 2027 Acme. Todos los derechos reservados.");
});
