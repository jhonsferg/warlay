import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createStatusFeed,
  idleStatus,
  pingLabel,
  pingTone,
  uptimeMinutes,
  type NetworkStatus,
} from "./networkStatus.ts";

test("ping tone goes from good to playable to bad at 80 and 150 ms", () => {
  assert.equal(pingTone(20), "success");
  assert.equal(pingTone(80), "success");
  assert.equal(pingTone(81), "warning");
  assert.equal(pingTone(150), "warning");
  assert.equal(pingTone(151), "danger");
});

test("ping label rounds, never shows 0 ms and hides what is not a ping", () => {
  assert.deepEqual(pingLabel(42.6), { text: "43 ms", tone: "success" });
  assert.deepEqual(pingLabel(0), { text: "1 ms", tone: "success" });
  assert.deepEqual(pingLabel(220), { text: "220 ms", tone: "danger" });
  assert.equal(pingLabel(null), null);
  assert.equal(pingLabel(undefined), null);
  assert.equal(pingLabel(Number.NaN), null);
  assert.equal(pingLabel(-5), null);
});

function status(over: Partial<NetworkStatus> = {}): NetworkStatus {
  return { ...idleStatus(), ...over };
}

/** A fake backend: records subscriptions and lets the test push statuses. */
function fakeBackend(current: NetworkStatus | Error = status()) {
  const handlers = new Set<(s: NetworkStatus) => void>();
  const backend = {
    fetches: 0,
    subscribed: 0,
    unsubscribed: 0,
    push: (s: NetworkStatus) => handlers.forEach((h) => h(s)),
    listeners: () => handlers.size,
    deps: {
      fetch: async () => {
        backend.fetches += 1;
        if (current instanceof Error) throw current;
        return current;
      },
      listen: async (handler: (s: NetworkStatus) => void) => {
        backend.subscribed += 1;
        handlers.add(handler);
        return () => {
          backend.unsubscribed += 1;
          handlers.delete(handler);
        };
      },
    },
  };
  return backend;
}

test("idle status is empty and has LAN discovery on", () => {
  const s = idleStatus();
  assert.equal(s.meshPeers.length, 0);
  assert.equal(s.tunnelActive, false);
  assert.equal(s.hubConnections, 0);
  assert.equal(s.lanDiscoveryEnabled, true);
  assert.notEqual(idleStatus().meshPeers, s.meshPeers, "each call returns a fresh array");
});

test("uptime adds the time since the status was read", () => {
  const s = status({ uptimeSecs: 120 });
  assert.equal(uptimeMinutes(s, 1_000, 1_000), 2);
  assert.equal(uptimeMinutes(s, 1_000, 61_000), 3);
  assert.equal(uptimeMinutes(s, 5_000, 1_000), 2, "a clock step back never subtracts");
});

test("starting reads the status once and then follows pushes", async () => {
  const backend = fakeBackend(status({ hubConnections: 1 }));
  const seen: NetworkStatus[] = [];
  const feed = createStatusFeed(backend.deps, (s) => seen.push(s));

  await feed.start();
  assert.equal(backend.fetches, 1);
  assert.deepEqual(
    seen.map((s) => s.hubConnections),
    [1],
  );

  backend.push(status({ hubConnections: 2, tunnelActive: true }));
  assert.equal(seen.length, 2);
  assert.equal(seen[1]?.tunnelActive, true);
  assert.equal(backend.fetches, 1, "no polling: a push does not trigger another read");
});

test("two users share one subscription and it ends with the last one", async () => {
  const backend = fakeBackend();
  const feed = createStatusFeed(backend.deps, () => {});

  await feed.start();
  await feed.start();
  assert.equal(backend.subscribed, 1);
  assert.equal(backend.fetches, 1);

  feed.stop();
  assert.equal(backend.listeners(), 1, "still used by the other screen");
  feed.stop();
  assert.equal(backend.listeners(), 0);
  assert.equal(backend.unsubscribed, 1);
});

test("stopping an unstarted feed, or too many times, is harmless", async () => {
  const backend = fakeBackend();
  const feed = createStatusFeed(backend.deps, () => {});
  feed.stop();
  await feed.start();
  feed.stop();
  feed.stop();
  assert.equal(backend.unsubscribed, 1);
  await feed.start();
  assert.equal(backend.subscribed, 2, "can be started again afterwards");
});

test("a read that finishes after a push does not overwrite the newer status", async () => {
  let release: (s: NetworkStatus) => void = () => {};
  const slow = new Promise<NetworkStatus>((resolve) => (release = resolve));
  const handlers: Array<(s: NetworkStatus) => void> = [];
  const seen: number[] = [];
  const feed = createStatusFeed(
    {
      fetch: () => slow,
      listen: async (h) => {
        handlers.push(h);
        return () => {};
      },
    },
    (s) => seen.push(s.hubConnections),
  );

  const started = feed.start();
  await new Promise((r) => setImmediate(r)); // subscribed, read still pending
  handlers[0]?.(status({ hubConnections: 5 })); // newer, pushed while the read is pending
  release(status({ hubConnections: 1 })); // older
  await started;

  assert.deepEqual(seen, [5]);
});

test("a failed read is tolerated and pushes still arrive", async () => {
  const backend = fakeBackend(new Error("agent not ready"));
  const seen: NetworkStatus[] = [];
  const feed = createStatusFeed(backend.deps, (s) => seen.push(s));

  await feed.start();
  assert.equal(seen.length, 0);
  backend.push(status({ tunnelActive: true }));
  assert.equal(seen.length, 1);
});

test("stopping while still subscribing releases the subscription", async () => {
  const backend = fakeBackend();
  const seen: NetworkStatus[] = [];
  const feed = createStatusFeed(backend.deps, (s) => seen.push(s));

  const started = feed.start();
  feed.stop(); // before the listen() promise settles
  await started;

  assert.equal(backend.listeners(), 0);
  assert.equal(seen.length, 0, "a stopped feed reports nothing");
});

test("pushes after the last stop are ignored", async () => {
  const backend = fakeBackend();
  const seen: NetworkStatus[] = [];
  const feed = createStatusFeed(backend.deps, (s) => seen.push(s));
  await feed.start();
  const before = seen.length;
  feed.stop();
  backend.push(status({ tunnelActive: true }));
  assert.equal(seen.length, before);
});
