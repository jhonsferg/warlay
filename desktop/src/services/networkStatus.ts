// The agent's network status (P2P links, game tunnel, hub connections).
//
// The agent runs inside this app, so reading it is an in-process call (a
// Tauri `invoke`), not a network request. Instead of polling it, screens read
// it once and then follow the `network://status` event the backend emits only
// when something changes. This module holds the pure logic for that, so it
// can be unit-tested without Vue or Tauri.

export interface PeerLinkStatus {
  clientId: string;
  isHost: boolean;
  p2pAddr: string | null;
  state: "not_attempted" | "connecting" | "connected" | "relayed" | "failed";
  /** Round-trip time of the live link in ms (the player's ping); null with no connection. */
  rttMs: number | null;
}

export type PingTone = "success" | "warning" | "danger";

/** How a ping reads at a glance: good up to 80 ms, playable up to 150 ms. */
export function pingTone(ms: number): PingTone {
  if (ms <= 80) return "success";
  if (ms <= 150) return "warning";
  return "danger";
}

/** The ping to show next to a player, or null when there is nothing to show
 * (no live link yet, or the status predates pings). */
export function pingLabel(rttMs: number | null | undefined): { text: string; tone: PingTone } | null {
  if (rttMs == null || !Number.isFinite(rttMs) || rttMs < 0) return null;
  const ms = Math.max(1, Math.round(rttMs)); // a live link is never "0 ms"
  return { text: `${ms} ms`, tone: pingTone(ms) };
}

export interface NetworkStatus {
  uptimeSecs: number;
  hubConnections: number;
  tunnelActive: boolean;
  lanDiscoveryEnabled: boolean;
  meshPeers: PeerLinkStatus[];
}

/** What the screens show before the first status arrives. */
export function idleStatus(): NetworkStatus {
  return { uptimeSecs: 0, hubConnections: 0, tunnelActive: false, lanDiscoveryEnabled: true, meshPeers: [] };
}

/** Whole minutes the agent has been up, counting time since `receivedAtMs`
 * (the status only carries the uptime it had when it was read). */
export function uptimeMinutes(status: NetworkStatus, receivedAtMs: number, nowMs: number): number {
  const extra = Math.max(0, nowMs - receivedAtMs) / 1000;
  return Math.round((status.uptimeSecs + extra) / 60);
}

export interface StatusFeedDeps {
  /** Reads the current status once. */
  fetch: () => Promise<NetworkStatus | undefined>;
  /** Subscribes to pushed changes; resolves to the unsubscribe function. */
  listen: (handler: (status: NetworkStatus) => void) => Promise<() => void>;
}

/**
 * Keeps `onStatus` up to date: one read when started, then every pushed
 * change. Starts are counted, so several screens can share one subscription;
 * it is released when the last of them stops.
 */
export function createStatusFeed(deps: StatusFeedDeps, onStatus: (status: NetworkStatus) => void) {
  let users = 0;
  let generation = 0;
  let unlisten: (() => void) | null = null;
  let pushed = false;

  async function start(): Promise<void> {
    users += 1;
    if (users > 1) return;
    const mine = ++generation;
    pushed = false;

    const off = await deps.listen((status) => {
      if (mine !== generation) return;
      pushed = true;
      onStatus(status);
    });
    if (mine !== generation) {
      off(); // stopped (and maybe restarted) while we were subscribing
      return;
    }
    unlisten = off;

    try {
      const status = await deps.fetch();
      // A pushed status is newer than a read that was still in flight.
      if (status && !pushed && mine === generation) onStatus(status);
    } catch {
      /* the next pushed change brings the status */
    }
  }

  function stop(): void {
    if (users === 0) return;
    users -= 1;
    if (users > 0) return;
    generation += 1;
    unlisten?.();
    unlisten = null;
  }

  return { start, stop };
}
