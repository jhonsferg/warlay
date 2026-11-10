// How the list of open sessions reads: a device name worked out from the user
// agent the session signed in with, and how long ago it was last used.

/** What a session looks like to the player. */
export interface SessionView {
  id: string;
  userAgent: string;
  /** `password` or `google`. */
  method: string;
  createdAt: string;
  lastUsedAt: string;
  /** The session this very app is using. */
  current: boolean;
}

/** "Edge en Windows", "Chrome en Android"... from a user-agent string. */
export function deviceName(userAgent: string): string {
  const ua = userAgent.trim();
  if (!ua) return "Dispositivo desconocido";
  // Order matters: Edge and Opera also say Chrome, Chrome also says Safari.
  const browser = /Edg(e|A|iOS)?\//.test(ua)
    ? "Edge"
    : /OPR\/|Opera/.test(ua)
      ? "Opera"
      : /Firefox\//.test(ua)
        ? "Firefox"
        : /Chrome\/|CriOS\//.test(ua)
          ? "Chrome"
          : /Safari\//.test(ua)
            ? "Safari"
            : null;
  const os = /Windows/.test(ua)
    ? "Windows"
    : /Android/.test(ua)
      ? "Android"
      : /iPhone|iPad|iOS/.test(ua)
        ? "iOS"
        : /Mac OS X|Macintosh/.test(ua)
          ? "macOS"
          : /Linux|X11/.test(ua)
            ? "Linux"
            : null;
  if (browser && os) return `${browser} en ${os}`;
  return browser ?? os ?? "Dispositivo desconocido";
}

export function methodLabel(method: string): string {
  return method === "google" ? "Google" : "Contraseña";
}

/** "ahora", "hace 5 min", "hace 3 h", "hace 2 días", or the date for older ones. */
export function agoText(iso: string, now: number): string {
  const seconds = Math.max(0, Math.floor((now - new Date(iso).getTime()) / 1000));
  if (seconds < 90) return "ahora";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `hace ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `hace ${hours} h`;
  const days = Math.round(hours / 24);
  if (days < 30) return `hace ${days} ${days === 1 ? "día" : "días"}`;
  return new Date(iso).toLocaleDateString("es", { day: "numeric", month: "short", year: "numeric" });
}
