// Telling "the server is not answering" from "the server says no". A server
// that is being replaced (or a proxy in front of it answering 502/503 for a few
// seconds) says nothing about the user's session: signing them out for it would
// also tear down the room and the game they are playing. Pure, so it is unit
// tested.

/** The server did not answer in a usable way: down, restarting, overloaded. */
export function isServerUnavailable(status: number): boolean {
  return status === 0 || status === 408 || status === 425 || status === 429 || status >= 500;
}

export type RefreshVerdict = "refreshed" | "expired" | "retry";

/** What the answer of the token refresh means for the stored session: only the
 * server rejecting the refresh token ends it. */
export function refreshVerdict(status: number): RefreshVerdict {
  if (status >= 200 && status < 300) return "refreshed";
  return isServerUnavailable(status) ? "retry" : "expired";
}

/** Waits before each new try to restore the session at startup. */
export const RESTORE_RETRY_MS = [1000, 2000, 4000, 6000];
