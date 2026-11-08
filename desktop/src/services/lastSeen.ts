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

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

const MONTHS = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

/** `8 oct 2026` -- spelled out so it does not depend on the runtime's ICU data. */
export function shortDate(at: Date): string {
  return `${at.getDate()} ${MONTHS[at.getMonth()]} ${at.getFullYear()}`;
}

export function formatLastSeen(iso: string | null | undefined, now: number = Date.now()): string {
  if (!iso) return "Desconectado";
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return "Desconectado";

  // A timestamp ahead of our clock (skew) counts as "just now", never negative.
  const age = Math.max(0, now - at.getTime());
  if (age < MINUTE) return "Última conexión: hace unos segundos";
  if (age < HOUR) return `Última conexión: hace ${plural(Math.floor(age / MINUTE), "minuto", "minutos")}`;
  if (age < DAY) return `Última conexión: hace ${plural(Math.floor(age / HOUR), "hora", "horas")}`;
  if (age < WEEK) return `Última conexión: hace ${plural(Math.floor(age / DAY), "día", "días")}`;
  if (age < MONTH) return `Última conexión: hace ${plural(Math.floor(age / WEEK), "semana", "semanas")}`;
  return `Última conexión: ${shortDate(at)}`;
}

export type PresenceLike = {
  status: "offline" | "online" | "in_room" | "in_game";
  roomId: string | null;
  lastSeenAt?: string | null;
};

const STATUS_TEXT = { online: "En línea", in_room: "En una sala", in_game: "En partida" } as const;

/** What to print under a person's name: their status, or when they were last here. */
export function lastSeenText(presence: PresenceLike, now: number = Date.now()): string {
  if (presence.status === "offline") return formatLastSeen(presence.lastSeenAt, now);
  return STATUS_TEXT[presence.status];
}
