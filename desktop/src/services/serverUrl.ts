// Where the Warlay server is. The address is NOT in the source code: it is
// injected when the app is built (`VITE_WARLAY_MAIN_URL`, from a GitHub secret
// in CI or from the environment / a `.env` file locally), so a fork points at
// its own server and the repository does not publish ours. This module holds
// the pure rules so they can be unit-tested, and is shared by the app and by
// the build (vite.config.ts), which refuses to build without a valid value.

/** What a developer gets from `npm run dev` when nothing is configured. */
export const DEV_SERVER_URL = "http://127.0.0.1:8080";

/** The build variable that carries the server address. */
export const SERVER_URL_VAR = "VITE_WARLAY_MAIN_URL";

/**
 * A usable server base URL (`http(s)://host[:port]`, no path, query or
 * credentials), without a trailing slash; null when `raw` is not one.
 */
export function parseServerUrl(raw: string | undefined | null): string | null {
  const text = (raw ?? "").trim();
  if (!text) return null;
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  if (!url.hostname || url.username || url.password || url.search || url.hash) return null;
  if (url.pathname !== "/" && url.pathname !== "") return null;
  return `${url.protocol}//${url.host}`;
}

/** Why `raw` cannot be used, in words for whoever is building the app. */
export function serverUrlProblem(raw: string | undefined | null): string | null {
  if (parseServerUrl(raw) !== null) return null;
  const missing = !(raw ?? "").trim();
  return missing
    ? `${SERVER_URL_VAR} is not set. The server address is injected at build time: set it as an environment ` +
        `variable or in desktop/.env (see desktop/.env.example); in GitHub Actions it comes from the ` +
        `WARLAY_MAIN_URL secret.`
    : `${SERVER_URL_VAR} must be a plain http(s) address such as https://warlay.example.com ` +
        `(no path, query or credentials); got "${raw}".`;
}

/**
 * The address the running app talks to: the injected one, or, only while
 * developing, a local server. A production build never gets here without
 * one (the build fails first); if it does, failing loudly beats talking to
 * the wrong place.
 */
export function resolveServerUrl(injected: string | undefined, dev: boolean): string {
  const parsed = parseServerUrl(injected);
  if (parsed) return parsed;
  if (dev && !(injected ?? "").trim()) return DEV_SERVER_URL;
  throw new Error(serverUrlProblem(injected) ?? "invalid server address");
}

/** Older versions let the user type a server address in Ajustes and kept it
 * under this key. The address is now fixed by the build and nothing reads the
 * key any more, so a leftover value is removed instead of lingering. */
export const LEGACY_SERVER_OVERRIDE_KEY = "warlay.mainServer";

export function removeLegacyServerOverride(storage: Pick<Storage, "removeItem">): void {
  storage.removeItem(LEGACY_SERVER_OVERRIDE_KEY);
}
