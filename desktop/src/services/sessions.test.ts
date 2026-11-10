import assert from "node:assert/strict";
import { test } from "node:test";
import { agoText, deviceName, methodLabel } from "./sessions.ts";

test("a device is named from its browser and system", () => {
  const edge =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36 Edg/130.0";
  assert.equal(deviceName(edge), "Edge en Windows");
  const chrome =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
  assert.equal(deviceName(chrome), "Chrome en Windows");
  assert.equal(
    deviceName("Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0"),
    "Firefox en Linux",
  );
  assert.equal(
    deviceName(
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
    ),
    "Safari en macOS",
  );
  assert.equal(
    deviceName("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36"),
    "Chrome en Android",
  );
});

test("an unknown or missing user agent still reads as something", () => {
  assert.equal(deviceName(""), "Dispositivo desconocido");
  assert.equal(deviceName("   "), "Dispositivo desconocido");
  assert.equal(deviceName("curl/8.4.0"), "Dispositivo desconocido");
  assert.equal(deviceName("Mozilla/5.0 (Windows NT 10.0)"), "Windows");
});

test("the sign-in method has a name", () => {
  assert.equal(methodLabel("google"), "Google");
  assert.equal(methodLabel("password"), "Contraseña");
});

test("time since last use reads naturally", () => {
  const now = Date.parse("2026-11-10T12:00:00Z");
  const ago = (ms: number) => new Date(now - ms).toISOString();
  assert.equal(agoText(ago(10_000), now), "ahora");
  assert.equal(agoText(ago(5 * 60_000), now), "hace 5 min");
  assert.equal(agoText(ago(3 * 3_600_000), now), "hace 3 h");
  assert.equal(agoText(ago(24 * 3_600_000), now), "hace 1 día");
  assert.equal(agoText(ago(3 * 24 * 3_600_000), now), "hace 3 días");
  assert.match(agoText(ago(90 * 24 * 3_600_000), now), /\d{4}/);
  // A clock a little behind the server's never shows a negative time.
  assert.equal(agoText(new Date(now + 5_000).toISOString(), now), "ahora");
});
