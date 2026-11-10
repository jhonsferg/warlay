// Why a player can be reported. The ids are what the server accepts; the
// labels are what the player reads.

export const REPORT_REASONS = [
  { id: "harassment", label: "Acoso o insultos" },
  { id: "spam", label: "Spam o publicidad" },
  { id: "inappropriate", label: "Contenido inapropiado" },
  { id: "cheating", label: "Trampas o abuso del juego" },
  { id: "impersonation", label: "Se hace pasar por otra persona" },
  { id: "other", label: "Otro motivo" },
] as const;

export type ReportReason = (typeof REPORT_REASONS)[number]["id"];

/** Longest explanation the server stores. */
export const REPORT_DETAILS_MAX = 500;

export function isReportReason(value: string): value is ReportReason {
  return REPORT_REASONS.some((r) => r.id === value);
}

/** Whether a report with this reason and explanation can be sent. */
export function canSendReport(reason: string, details: string): boolean {
  return isReportReason(reason) && details.trim().length <= REPORT_DETAILS_MAX;
}
