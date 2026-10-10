import type { PresenceStatus } from "../services/types";

export function presenceLabel(status: PresenceStatus): string {
  switch (status) {
    case "online":
      return "En línea";
    case "in_room":
      return "En una sala";
    case "in_game":
      return "En partida";
    default:
      return "Desconectado";
  }
}

/** Order used when grouping people by presence (most "available" first). */
export const PRESENCE_ORDER: PresenceStatus[] = ["in_game", "in_room", "online", "offline"];
