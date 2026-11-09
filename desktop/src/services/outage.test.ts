import assert from "node:assert/strict";
import { test } from "node:test";
import { isServerUnavailable, refreshVerdict, RESTORE_RETRY_MS } from "./outage.ts";

test("a server that is down, restarting or overloaded is unavailable", () => {
  for (const status of [0, 408, 425, 429, 500, 502, 503, 504]) assert.ok(isServerUnavailable(status), String(status));
});

test("an answer about the request itself is not an outage", () => {
  for (const status of [200, 204, 400, 401, 403, 404, 409, 422])
    assert.ok(!isServerUnavailable(status), String(status));
});

test("a refresh answered 5xx keeps the session; only a rejection ends it", () => {
  assert.equal(refreshVerdict(200), "refreshed");
  assert.equal(refreshVerdict(502), "retry");
  assert.equal(refreshVerdict(503), "retry");
  assert.equal(refreshVerdict(429), "retry");
  assert.equal(refreshVerdict(401), "expired");
  assert.equal(refreshVerdict(403), "expired");
  assert.equal(refreshVerdict(400), "expired");
});

test("the startup retries cover an outage of several seconds", () => {
  const total = RESTORE_RETRY_MS.reduce((a, b) => a + b, 0);
  assert.ok(total >= 10_000);
});
