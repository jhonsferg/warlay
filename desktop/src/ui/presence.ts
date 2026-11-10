import type { PresenceStatus } from "../services/types";
import { t } from "../services/i18n";

export function presenceLabel(status: PresenceStatus): string {
  switch (status) {
    case "online":
      return t("En línea");
    case "in_room":
      return t("En una sala");
    case "in_game":
      return t("En partida");
    default:
      return t("Desconectado");
  }
}

/** Order used when grouping people by presence (most "available" first). */
export const PRESENCE_ORDER: PresenceStatus[] = ["in_game", "in_room", "online", "offline"];
