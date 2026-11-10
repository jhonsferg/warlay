import { locale, t, tx } from "./i18n.ts";

/**
 * "Última conexión: hace 2 minutos", like Messenger: relative up to a month,
 * then just the date. Pure (the clock is a parameter) so it is unit tested.
 */
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const WEEK = 7 * DAY;
/** From here on a relative age stops being useful: show the date. */
const MONTH = 30 * DAY;

const ago = (n: number, one: string, many: string) => (n === 1 ? t(one) : t(many, { n }));

const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
const MONTHS_EN = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `8 oct 2026` -- spelled out so it does not depend on the runtime's ICU data. */
export function shortDate(at: Date): string {
  const months = locale.value === "en" ? MONTHS_EN : MONTHS;
  return `${at.getDate()} ${months[at.getMonth()]} ${at.getFullYear()}`;
}

export function formatLastSeen(iso: string | null | undefined, now: number = Date.now()): string {
  if (!iso) return t("Desconectado");
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return t("Desconectado");

  // A timestamp ahead of our clock (skew) counts as "just now", never negative.
  const age = Math.max(0, now - at.getTime());
  if (age < MINUTE) return t("Última conexión: hace unos segundos");
  if (age < HOUR)
    return ago(Math.floor(age / MINUTE), tx("Última conexión: hace 1 minuto"), tx("Última conexión: hace {n} minutos"));
  if (age < DAY)
    return ago(Math.floor(age / HOUR), tx("Última conexión: hace 1 hora"), tx("Última conexión: hace {n} horas"));
  if (age < WEEK)
    return ago(Math.floor(age / DAY), tx("Última conexión: hace 1 día"), tx("Última conexión: hace {n} días"));
  if (age < MONTH)
    return ago(Math.floor(age / WEEK), tx("Última conexión: hace 1 semana"), tx("Última conexión: hace {n} semanas"));
  return t("Última conexión: {date}", { date: shortDate(at) });
}

export type PresenceLike = {
  status: "offline" | "online" | "in_room" | "in_game";
  roomId: string | null;
  lastSeenAt?: string | null;
};

const STATUS_TEXT = { online: tx("En línea"), in_room: tx("En una sala"), in_game: tx("En partida") } as const;

/** What to print under a person's name: their status, or when they were last here. */
export function lastSeenText(presence: PresenceLike, now: number = Date.now()): string {
  if (presence.status === "offline") return formatLastSeen(presence.lastSeenAt, now);
  return t(STATUS_TEXT[presence.status]);
}
