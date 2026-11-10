import { localeTag, t } from "./i18n.ts";
// What the "Acerca de" panel shows about this installation. The values come
// from the desktop shell (`get_app_info`); this module only formats them, so
// it can be unit-tested without Vue or Tauri.

export interface AppInfo {
  name: string;
  version: string;
  /** Short commit the build was made from. */
  build: string;
  /** When the build was made (Unix seconds): the release date. */
  releasedAt: number;
  /** When this version was installed or last updated (Unix seconds). */
  installedAt: number;
}

/** A Unix timestamp (seconds) as a long local date, or a dash when unknown. */
export function formatDate(unixSeconds: number | null | undefined, locale = localeTag()): string {
  if (!unixSeconds || !Number.isFinite(unixSeconds) || unixSeconds <= 0) return "—";
  return new Date(unixSeconds * 1000).toLocaleDateString(locale, { day: "numeric", month: "long", year: "numeric" });
}

export function copyrightLine(year: number, name = "Warlay"): string {
  return t("© {year} {name}. Todos los derechos reservados.", { year, name });
}
