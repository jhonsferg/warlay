import type { ServerConnectionStatus } from "../types";

/** Shared by every place that shows a connection-status BaseBadge (the
 * server rail and the rooms view) so the color mapping can't drift between
 * the two. Each state gets a genuinely different hue -- "connecting" reads
 * as a cool, in-progress steel-blue, not a dimmer version of "connected"'s
 * green. */
export function connectionStatusTone(
  status: ServerConnectionStatus | undefined,
): "success" | "danger" | "amber" | "steel" | "neutral" {
  switch (status) {
    case "connected":
      return "success";
    case "connecting":
      return "steel";
    case "passwordRequired":
      return "amber";
    case "rejected":
      return "danger";
    default:
      return "neutral";
  }
}
