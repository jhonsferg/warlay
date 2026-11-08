import assert from "node:assert/strict";
import { test } from "node:test";
import {
  createUpdater,
  formatBytes,
  percent,
  releaseHighlights,
  type UpdateProgress,
  type UpdateState,
  type UpdateStatus,
} from "./updates.ts";

function status(over: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    current: "0.9.0",
    latest: "0.10.0",
    available: true,
    notes: "",
    publishedAt: "2026-11-09T10:00:00Z",
    pageUrl: "https://github.com/jhonsferg/warlay/releases/latest",
    sizeBytes: 1000,
    canSelfUpdate: true,
    ...over,
  };
}

/** A fake shell: records calls and lets the test drive progress. */
function harness(over: { check?: () => Promise<UpdateStatus>; install?: () => Promise<void> } = {}) {
  const seen: UpdateState[] = [];
  let push: (p: UpdateProgress) => void = () => {};
  const calls = { check: 0, install: 0, subscribed: 0, unsubscribed: 0 };
  const updater = createUpdater(
    {
      check: async () => {
        calls.check += 1;
        return (over.check ?? (async () => status()))();
      },
      install: async () => {
        calls.install += 1;
        await (over.install ?? (async () => {}))();
      },
      onProgress: async (handler) => {
        calls.subscribed += 1;
        push = handler;
        return () => {
          calls.unsubscribed += 1;
        };
      },
    },
    (s) => seen.push(s),
  );
  return { updater, seen, calls, push: (p: UpdateProgress) => push(p), phases: () => seen.map((s) => s.phase) };
}

test("percent is floored, clamped and unknown without a size", () => {
  assert.equal(percent(null), null);
  assert.equal(percent({ receivedBytes: 5, totalBytes: 0 }), null);
  assert.equal(percent({ receivedBytes: 1, totalBytes: 3 }), 33);
  assert.equal(percent({ receivedBytes: 10, totalBytes: 10 }), 100);
  assert.equal(percent({ receivedBytes: 99, totalBytes: 10 }), 100);
});

test("sizes read in B, KB, MB and GB, and an unknown size shows a dash", () => {
  assert.equal(formatBytes(0), "—");
  assert.equal(formatBytes(Number.NaN), "—");
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(2048), "2 KB");
  assert.equal(formatBytes(40 * 1024 * 1024), "40.0 MB");
  assert.equal(formatBytes(3 * 1024 ** 3), "3.00 GB");
});

test("release highlights keep the list items, without markdown or hashes", () => {
  const notes = [
    "## Downloads",
    "| Component | Platform | File |",
    "## Changes",
    "### Features",
    "- **desktop**: muestra el ping de cada jugador (abc1234)",
    "* corrige [el anillo](https://x.test/y) de voz",
    "",
    "- `fix`: algo mas",
  ].join("\r\n");
  assert.deepEqual(releaseHighlights(notes), [
    "desktop: muestra el ping de cada jugador",
    "corrige el anillo de voz",
    "fix: algo mas",
  ]);
});

test("release highlights respect the limit and tolerate empty notes", () => {
  assert.deepEqual(releaseHighlights(""), []);
  const many = Array.from({ length: 20 }, (_, i) => `- cambio ${i}`).join("\n");
  assert.equal(releaseHighlights(many, 3).length, 3);
});

test("checking finds an update", async () => {
  const h = harness();
  await h.updater.check();
  assert.deepEqual(h.phases(), ["checking", "available"]);
  assert.equal(h.updater.state().status?.latest, "0.10.0");
});

test("checking with nothing new says the client is up to date", async () => {
  const h = harness({ check: async () => status({ available: false, latest: "0.9.0" }) });
  await h.updater.check();
  assert.equal(h.updater.state().phase, "uptodate");
});

test("a failed check shows the error and can be tried again", async () => {
  let fail = true;
  const h = harness({
    check: async () => {
      if (fail) throw new Error("sin conexión");
      return status();
    },
  });
  await h.updater.check();
  assert.equal(h.updater.state().phase, "error");
  assert.equal(h.updater.state().error, "sin conexión");
  fail = false;
  await h.updater.check();
  assert.equal(h.updater.state().phase, "available");
  assert.equal(h.updater.state().error, null);
});

test("a second click while checking does nothing", async () => {
  let release: (s: UpdateStatus) => void = () => {};
  const h = harness({ check: () => new Promise<UpdateStatus>((r) => (release = r)) });
  const first = h.updater.check();
  await new Promise((r) => setImmediate(r));
  await h.updater.check();
  assert.equal(h.calls.check, 1);
  release(status());
  await first;
});

test("installing without an offered update does nothing", async () => {
  const h = harness();
  await h.updater.install();
  assert.equal(h.calls.install, 0);
});

test("a portable copy cannot install by itself", async () => {
  const h = harness({ check: async () => status({ canSelfUpdate: false }) });
  await h.updater.check();
  await h.updater.install();
  assert.equal(h.calls.install, 0);
  assert.equal(h.updater.state().phase, "available");
});

test("installing follows the download and ends installing", async () => {
  let finish: () => void = () => {};
  const h = harness({ install: () => new Promise<void>((r) => (finish = r)) });
  await h.updater.check();
  const installing = h.updater.install();
  await new Promise((r) => setImmediate(r));

  assert.equal(h.updater.state().phase, "downloading");
  h.push({ receivedBytes: 400, totalBytes: 1000 });
  assert.equal(percent(h.updater.state().progress), 40);
  assert.equal(h.updater.state().phase, "downloading");
  h.push({ receivedBytes: 1000, totalBytes: 1000 });
  assert.equal(h.updater.state().phase, "installing", "verified, about to run the installer");

  finish();
  await installing;
  assert.equal(h.calls.unsubscribed, 1, "progress is released");
});

test("a failed install goes back to offering the update, with the reason", async () => {
  const h = harness({
    install: async () => {
      throw new Error("la descarga no coincide");
    },
  });
  await h.updater.check();
  await h.updater.install();
  assert.equal(h.updater.state().phase, "available");
  assert.equal(h.updater.state().error, "la descarga no coincide");
  assert.equal(h.updater.state().progress, null);
  assert.equal(h.calls.unsubscribed, 1);
  await h.updater.install();
  assert.equal(h.calls.install, 2, "it can be retried");
});

test("late progress after a failed install is ignored", async () => {
  const h = harness({
    install: async () => {
      throw new Error("x");
    },
  });
  await h.updater.check();
  await h.updater.install();
  const before = h.seen.length;
  h.push({ receivedBytes: 5, totalBytes: 10 });
  assert.equal(h.seen.length, before);
});
