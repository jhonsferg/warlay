import assert from "node:assert/strict";
import { test } from "node:test";
import { DEV_SERVER_URL, parseServerUrl, resolveServerUrl, serverUrlProblem } from "./serverUrl.ts";

test("a plain http(s) address is accepted and normalised", () => {
  assert.equal(parseServerUrl("https://warlay.example.com"), "https://warlay.example.com");
  assert.equal(parseServerUrl("  https://warlay.example.com/  "), "https://warlay.example.com");
  assert.equal(parseServerUrl("http://203.0.113.7:8080"), "http://203.0.113.7:8080");
  assert.equal(parseServerUrl("HTTPS://Warlay.Example.com:8443"), "https://warlay.example.com:8443");
});

test("anything else is rejected", () => {
  for (const bad of [
    "",
    "   ",
    undefined,
    null,
    "warlay.example.com",
    "ftp://warlay.example.com",
    "wss://warlay.example.com",
    "https://",
    "https://user:pass@warlay.example.com",
    "https://warlay.example.com/api",
    "https://warlay.example.com?x=1",
    "https://warlay.example.com#top",
    "not a url",
  ]) {
    assert.equal(parseServerUrl(bad), null, String(bad));
  }
});

test("the problem explains how to fix a missing or a malformed value", () => {
  assert.equal(serverUrlProblem("https://warlay.example.com"), null);
  const missing = serverUrlProblem(undefined) ?? "";
  assert.match(missing, /VITE_WARLAY_MAIN_URL/);
  assert.match(missing, /\.env/);
  assert.match(missing, /WARLAY_MAIN_URL secret/);
  assert.match(serverUrlProblem("warlay.example.com") ?? "", /got "warlay\.example\.com"/);
});

test("the running app uses the injected address", () => {
  assert.equal(resolveServerUrl("https://warlay.example.com/", false), "https://warlay.example.com");
  assert.equal(resolveServerUrl("https://warlay.example.com", true), "https://warlay.example.com");
});

test("without an address only development falls back to a local server", () => {
  assert.equal(resolveServerUrl(undefined, true), DEV_SERVER_URL);
  assert.equal(resolveServerUrl("", true), DEV_SERVER_URL);
  assert.throws(() => resolveServerUrl(undefined, false), /VITE_WARLAY_MAIN_URL is not set/);
  assert.throws(() => resolveServerUrl("", false), /not set/);
});

test("a malformed address fails even in development instead of hiding the mistake", () => {
  assert.throws(() => resolveServerUrl("warlay.example.com", true), /plain http\(s\) address/);
});
