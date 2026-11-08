import assert from "node:assert/strict";
import { test } from "node:test";
import { formatLastSeen, lastSeenText } from "./lastSeen.ts";

const NOW = Date.parse("2026-11-07T18:00:00Z");
const ago = (seconds: number) => new Date(NOW - seconds * 1000).toISOString();
const MIN = 60;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

test("under a minute", () => {
  assert.equal(formatLastSeen(ago(0), NOW), "Última conexión: hace unos segundos");
  assert.equal(formatLastSeen(ago(59), NOW), "Última conexión: hace unos segundos");
});

test("minutes, singular and plural", () => {
  assert.equal(formatLastSeen(ago(MIN), NOW), "Última conexión: hace 1 minuto");
  assert.equal(formatLastSeen(ago(2 * MIN + 10), NOW), "Última conexión: hace 2 minutos");
  assert.equal(formatLastSeen(ago(59 * MIN + 59), NOW), "Última conexión: hace 59 minutos");
});

test("hours, singular and plural", () => {
  assert.equal(formatLastSeen(ago(HOUR), NOW), "Última conexión: hace 1 hora");
  assert.equal(formatLastSeen(ago(5 * HOUR + 30 * MIN), NOW), "Última conexión: hace 5 horas");
  assert.equal(formatLastSeen(ago(23 * HOUR + 59 * MIN), NOW), "Última conexión: hace 23 horas");
});

test("days, singular and plural", () => {
  assert.equal(formatLastSeen(ago(DAY), NOW), "Última conexión: hace 1 día");
  assert.equal(formatLastSeen(ago(3 * DAY + 2 * HOUR), NOW), "Última conexión: hace 3 días");
  assert.equal(formatLastSeen(ago(6 * DAY + 23 * HOUR), NOW), "Última conexión: hace 6 días");
});

test("from a week on it is counted in weeks", () => {
  assert.equal(formatLastSeen(ago(7 * DAY), NOW), "Última conexión: hace 1 semana");
  assert.equal(formatLastSeen(ago(13 * DAY), NOW), "Última conexión: hace 1 semana");
  assert.equal(formatLastSeen(ago(14 * DAY), NOW), "Última conexión: hace 2 semanas");
  assert.equal(formatLastSeen(ago(29 * DAY), NOW), "Última conexión: hace 4 semanas");
});

test("from a month on it is just the date", () => {
  assert.equal(formatLastSeen(ago(30 * DAY), NOW), "Última conexión: 8 oct 2026");
  assert.equal(formatLastSeen("2026-01-05T10:00:00Z", NOW), "Última conexión: 5 ene 2026");
  assert.equal(formatLastSeen("2025-12-31T10:00:00Z", NOW), "Última conexión: 31 dic 2025");
});

test("a timestamp slightly in the future (clock skew) reads as just now, never negative", () => {
  assert.equal(formatLastSeen(ago(-20), NOW), "Última conexión: hace unos segundos");
  assert.equal(formatLastSeen(ago(-3 * HOUR), NOW), "Última conexión: hace unos segundos");
});

test("no timestamp (never connected) or an unparseable one falls back to Desconectado", () => {
  assert.equal(formatLastSeen(null, NOW), "Desconectado");
  assert.equal(formatLastSeen(undefined, NOW), "Desconectado");
  assert.equal(formatLastSeen("not a date", NOW), "Desconectado");
});

test("lastSeenText: only offline people get the last-seen text; the rest keep their status", () => {
  const offline = { status: "offline" as const, roomId: null, lastSeenAt: ago(2 * MIN) };
  assert.equal(lastSeenText(offline, NOW), "Última conexión: hace 2 minutos");
  assert.equal(lastSeenText({ status: "online", roomId: null, lastSeenAt: null }, NOW), "En línea");
  assert.equal(lastSeenText({ status: "in_room", roomId: "r", lastSeenAt: null }, NOW), "En una sala");
  assert.equal(lastSeenText({ status: "in_game", roomId: "r", lastSeenAt: null }, NOW), "En partida");
  assert.equal(lastSeenText({ status: "offline", roomId: null, lastSeenAt: null }, NOW), "Desconectado");
});
