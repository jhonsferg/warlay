import type { AppSettings } from "../types";

/** Turns the free-text "Argumentos de lanzamiento" field plus the
 * "Motor gráfico" selector into the actual argv passed to `launch_game` —
 * shared by `store/rooms.ts` and `store/lanMesh.ts` so the two launch paths
 * (hub-hosted room vs. LAN mesh) can't drift on how they build this. */
export function buildLaunchArgs(settings: AppSettings): string[] {
  const args = settings.gameArgs.trim().length > 0 ? settings.gameArgs.trim().split(/\s+/) : [];
  if (settings.graphicsApi) args.push("-graphicsapi", settings.graphicsApi);
  return args;
}
