import assert from "node:assert/strict";
import { test } from "node:test";
import { compareFriends, matchesQuery } from "./friendOrder.ts";

type F = Parameters<typeof compareFriends>[0];
const friend = (name: string, status: F["presence"]["status"], lastSeenAt: string | null = null, username = name.toLowerCase()): F => ({
  displayName: name,
  username,
  presence: { status, roomId: null, lastSeenAt },
});
const order = (list: F[]) => [...list].sort(compareFriends).map((f) => f.displayName);

test("people who are around come first: in game, in a room, online, then offline", () => {
  const list = [
    friend("Off", "offline"),
    friend("On", "online"),
    friend("Game", "in_game"),
    friend("Room", "in_room"),
  ];
  assert.deepEqual(order(list), ["Game", "Room", "On", "Off"]);
});

test("within a group, names are alphabetical (accents and case do not break it)", () => {
  assert.deepEqual(
    order([friend("zoe", "online"), friend("Ángel", "online"), friend("bruno", "online")]),
    ["Ángel", "bruno", "zoe"],
  );
});

test("offline friends: the most recently seen first, never-seen last", () => {
  const list = [
    friend("Old", "offline", "2026-01-01T00:00:00Z"),
    friend("Never", "offline", null),
    friend("Recent", "offline", "2026-11-07T17:59:00Z"),
    friend("Mid", "offline", "2026-10-01T00:00:00Z"),
  ];
  assert.deepEqual(order(list), ["Recent", "Mid", "Old", "Never"]);
});

test("offline friends seen at the same instant fall back to the name", () => {
  const at = "2026-11-07T10:00:00Z";
  assert.deepEqual(order([friend("B", "offline", at), friend("A", "offline", at)]), ["A", "B"]);
});

test("an unparseable timestamp counts as never seen", () => {
  assert.deepEqual(order([friend("Bad", "offline", "nope"), friend("Ok", "offline", "2026-01-01T00:00:00Z")]), ["Ok", "Bad"]);
});

test("search matches display name or username, ignoring case and accents", () => {
  const f = friend("Jaina Valiente", "online", null, "jaina");
  assert.equal(matchesQuery(f, ""), true);
  assert.equal(matchesQuery(f, "   "), true);
  assert.equal(matchesQuery(f, "val"), true);
  assert.equal(matchesQuery(f, "JAIN"), true);
  assert.equal(matchesQuery(friend("Ángel", "online", null, "angel99"), "angel"), true);
  assert.equal(matchesQuery(friend("Angel", "online", null, "x"), "ÁNGEL"), true);
  assert.equal(matchesQuery(f, "arthas"), false);
});
