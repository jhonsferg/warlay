import { localeTag, t } from "./i18n.ts";
// Client self-update (like Discord): look for a newer GitHub release, then
// download it, verify it and restart into it. The work is done by the desktop
// shell; this module holds the screen's logic (phases, progress, how the
// release notes read) so it can be unit-tested without Vue or Tauri.

export interface UpdateStatus {
  current: string;
  latest: string;
  available: boolean;
  notes: string;
  publishedAt: string;
  pageUrl: string;
  sizeBytes: number;
  /** An installed copy can replace itself; a portable one is updated by hand. */
  canSelfUpdate: boolean;
}

export interface UpdateProgress {
  receivedBytes: number;
  totalBytes: number;
}

export type UpdatePhase =
  | "idle" // nothing looked up yet
  | "checking"
  | "uptodate"
  | "available"
  | "downloading"
  | "installing" // downloaded and verified; the installer is about to run
  | "error";

export interface UpdateState {
  phase: UpdatePhase;
  status: UpdateStatus | null;
  progress: UpdateProgress | null;
  error: string | null;
}

export function initialUpdateState(): UpdateState {
  return { phase: "idle", status: null, progress: null, error: null };
}

/** Download completion 0..100, or null while the size is not known. */
export function percent(progress: UpdateProgress | null): number | null {
  if (!progress || progress.totalBytes <= 0) return null;
  return Math.min(100, Math.max(0, Math.floor((progress.receivedBytes / progress.totalBytes) * 100)));
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/**
 * The lines of a release's notes worth showing: the list items of the
 * changelog, without markdown, headings or the downloads table. At most
 * `limit` of them.
 */
export function releaseHighlights(notes: string, limit = 6): string[] {
  const out: string[] = [];
  for (const raw of notes.split(/\r?\n/)) {
    const match = /^\s*[-*]\s+(.*\S)\s*$/.exec(raw);
    if (!match) continue;
    const text = match[1]
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1") // [text](url) -> text
      .replace(/[`*_]/g, "")
      .replace(/\s+\(?[0-9a-f]{7,40}\)?$/i, "") // trailing commit hash
      .trim();
    if (text) out.push(text);
    if (out.length === limit) break;
  }
  return out;
}

export interface UpdaterDeps {
  /** Looks up the latest release (rejects with a message when it cannot). */
  check: () => Promise<UpdateStatus>;
  /** Downloads, verifies and installs; the app restarts, so it may never resolve. */
  install: () => Promise<void>;
  /** Subscribes to download progress; resolves to the unsubscribe function. */
  onProgress: (handler: (progress: UpdateProgress) => void) => Promise<() => void>;
}

const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * The update flow as a small state machine. `onChange` receives every new
 * state; calls that do not fit the current phase (a second click while
 * checking, installing without an update) are ignored.
 */
export function createUpdater(deps: UpdaterDeps, onChange: (state: UpdateState) => void) {
  let state = initialUpdateState();
  const set = (next: Partial<UpdateState>) => {
    state = { ...state, ...next };
    onChange(state);
  };

  async function check(): Promise<void> {
    if (state.phase === "checking" || state.phase === "downloading" || state.phase === "installing") return;
    set({ phase: "checking", error: null, progress: null });
    try {
      const status = await deps.check();
      set({ status, phase: status.available ? "available" : "uptodate" });
    } catch (e) {
      set({ phase: "error", error: messageOf(e) });
    }
  }

  async function install(): Promise<void> {
    if (state.phase !== "available" || !state.status?.canSelfUpdate) return;
    set({ phase: "downloading", error: null, progress: { receivedBytes: 0, totalBytes: state.status.sizeBytes } });
    let stop: (() => void) | null = null;
    try {
      stop = await deps.onProgress((progress) => {
        if (state.phase !== "downloading" && state.phase !== "installing") return;
        const done = progress.totalBytes > 0 && progress.receivedBytes >= progress.totalBytes;
        set({ progress, phase: done ? "installing" : "downloading" });
      });
      await deps.install();
      // Reaching here means the shell answered before quitting.
      set({ phase: "installing" });
    } catch (e) {
      // Nothing was installed: the update is still on offer.
      set({ phase: "available", progress: null, error: messageOf(e) });
    } finally {
      stop?.();
    }
  }

  return { check, install, state: () => state };
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** When the last check happened, as the label under the check button. */
export function formatLastCheck(at: number | null, now: number, locale = localeTag()): string {
  if (at === null || !Number.isFinite(at)) return t("Nunca");
  const ago = Math.max(0, now - at);
  if (ago < MINUTE) return t("Hace unos segundos");
  if (ago < HOUR) return t("Hace {n} min", { n: Math.floor(ago / MINUTE) });
  if (ago < 24 * HOUR) return t("Hace {n} h", { n: Math.floor(ago / HOUR) });
  return new Date(at).toLocaleString(locale, {
    day: "numeric",
    month: "long",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const LAST_CHECK_KEY = "warlay.updates.lastCheck";

/** Remembers the last successful check across restarts. */
export function readLastCheck(storage: Pick<Storage, "getItem">): number | null {
  const raw = storage.getItem(LAST_CHECK_KEY);
  const value = raw === null ? Number.NaN : Number(raw);
  return Number.isFinite(value) && value > 0 ? value : null;
}

export function saveLastCheck(storage: Pick<Storage, "setItem">, at: number): void {
  storage.setItem(LAST_CHECK_KEY, String(at));
}
