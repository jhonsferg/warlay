import type { ApiErrorBody, Tokens } from "./types";

const TOKENS_KEY = "warlay.tokens";
const SERVER_KEY = "warlay.mainServer";

/** The main server's base URL: a user override (Ajustes > Avanzado), the
 * build-time default, or a local server for development. */
export function mainServerUrl(): string {
  const override = localStorage.getItem(SERVER_KEY);
  const fallback = import.meta.env.VITE_WARLAY_MAIN_URL ?? "http://127.0.0.1:8080";
  return (override || fallback).replace(/\/+$/, "");
}

export function setMainServerUrl(url: string | null): void {
  if (url && url.trim()) localStorage.setItem(SERVER_KEY, url.trim());
  else localStorage.removeItem(SERVER_KEY);
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

let tokens: Tokens | null = loadTokens();
let refreshing: Promise<boolean> | null = null;
const listeners = new Set<(signedIn: boolean) => void>();

function loadTokens(): Tokens | null {
  try {
    const raw = localStorage.getItem(TOKENS_KEY);
    return raw ? (JSON.parse(raw) as Tokens) : null;
  } catch {
    return null;
  }
}

export function setTokens(next: Tokens | null): void {
  tokens = next;
  if (next) localStorage.setItem(TOKENS_KEY, JSON.stringify(next));
  else localStorage.removeItem(TOKENS_KEY);
  listeners.forEach((l) => l(next !== null));
}

export function accessToken(): string | null {
  return tokens?.accessToken ?? null;
}

export function hasSession(): boolean {
  return tokens !== null;
}

/** Called whenever the session starts or ends (including a failed refresh). */
export function onSessionChange(listener: (signedIn: boolean) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Exchanges the refresh token for a new pair; one refresh at a time. */
export function refreshTokens(): Promise<boolean> {
  if (!tokens) return Promise.resolve(false);
  refreshing ??= (async () => {
    try {
      const res = await fetch(`${mainServerUrl()}/api/v1/auth/refresh`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ refreshToken: tokens?.refreshToken }),
      });
      if (!res.ok) {
        setTokens(null);
        return false;
      }
      setTokens((await res.json()) as Tokens);
      return true;
    } catch {
      // Network trouble: keep the session, the caller will see the error.
      return false;
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

async function send(method: Method, path: string, body?: unknown, base = mainServerUrl()): Promise<Response> {
  const headers: Record<string, string> = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (tokens) headers.Authorization = `Bearer ${tokens.accessToken}`;
  return fetch(`${base}/api/v1${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

/** Calls the main server's API, refreshing the access token once on 401. */
/** Calls the main server's API (or, with `base`, a helper edge's), refreshing
 * the access token once on 401. */
export async function api<T>(method: Method, path: string, body?: unknown, base?: string): Promise<T> {
  let res: Response;
  try {
    res = await send(method, path, body, base);
    if (res.status === 401 && tokens && (await refreshTokens())) {
      res = await send(method, path, body, base);
    }
  } catch {
    throw new ApiError(0, "network", "No se pudo conectar con el servidor de Warlay");
  }
  if (res.ok) {
    const text = await res.text();
    return (text ? JSON.parse(text) : null) as T;
  }
  let error: ApiErrorBody = { error: "unknown", message: `Error ${res.status}` };
  try {
    error = (await res.json()) as ApiErrorBody;
  } catch {
    /* not JSON */
  }
  if (res.status === 401 && path !== "/auth/login") setTokens(null);
  throw new ApiError(res.status, error.error, error.message);
}

export function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}
