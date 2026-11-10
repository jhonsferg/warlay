import { tx } from "./i18n.ts";
// Warcraft III launch arguments as switches. The game's settings keep a plain
// string (`gameArgs`, passed as-is to the executable); this module turns that
// string into on/off options and back, so the screen needs no text box.
//
// Only arguments that are documented for the game are listed (see
// PCGamingWiki and the Hive Workshop command-line guide). Audio and graphics
// quality have no command-line switch: they live in the game's own options.

export type GameArgGroup = "display" | "graphics" | "behavior";

export interface GameArg {
  id: string;
  /** What is passed to the game, e.g. ["-graphicsapi", "OpenGL2"]. */
  tokens: string[];
  label: string;
  description: string;
  group: GameArgGroup;
  /** Options that cannot be on together with this one. */
  excludes?: string[];
}

export const GAME_ARG_GROUPS: { id: GameArgGroup; label: string }[] = [
  { id: "display", label: tx("Pantalla") },
  { id: "graphics", label: tx("Gráficos") },
  { id: "behavior", label: tx("Comportamiento") },
];

export const GAME_ARGS: GameArg[] = [
  {
    id: "window",
    tokens: ["-window"],
    label: tx("Modo ventana"),
    description: tx("Abre el juego en una ventana en vez de a pantalla completa."),
    group: "display",
    excludes: ["nativefullscr"],
  },
  {
    id: "nativefullscr",
    tokens: ["-nativefullscr"],
    label: tx("Pantalla completa exclusiva"),
    description: tx(
      "Usa la pantalla completa real. Desde la versión 1.28 el juego usa por defecto una ventana sin bordes; esta opción devuelve el modo clásico.",
    ),
    group: "display",
    excludes: ["window"],
  },
  {
    id: "opengl",
    tokens: ["-opengl"],
    label: tx("Renderizar con OpenGL"),
    description: tx(
      "Fuerza OpenGL en lugar de Direct3D. Útil si hay fallos gráficos o en equipos con Wine. Versiones 1.26 a 1.28.",
    ),
    group: "graphics",
    excludes: ["opengl2"],
  },
  {
    id: "opengl2",
    tokens: ["-graphicsapi", "OpenGL2"],
    label: tx("Renderizar con OpenGL (1.29 en adelante)"),
    description: tx("Forma actual de forzar OpenGL; reemplaza a la opción anterior en las versiones nuevas del juego."),
    group: "graphics",
    excludes: ["opengl"],
  },
  {
    id: "nowfpause",
    tokens: ["-nowfpause"],
    label: tx("No pausar al perder el foco"),
    description: tx("El juego sigue corriendo cuando cambias a otra ventana, en vez de pausarse."),
    group: "behavior",
  },
];

/** Splits on spaces, keeping "quoted text" together. */
function tokenize(args: string): string[] {
  return args.match(/"[^"]*"|\S+/g) ?? [];
}

export interface ParsedGameArgs {
  enabled: Set<string>;
  /** Arguments this screen has no switch for; they are kept untouched. */
  extra: string[];
}

export function parseGameArgs(args: string): ParsedGameArgs {
  const tokens = tokenize(args);
  const enabled = new Set<string>();
  const extra: string[] = [];
  // Longest options first, so "-graphicsapi OpenGL2" wins over a lone flag.
  const byLength = [...GAME_ARGS].sort((a, b) => b.tokens.length - a.tokens.length);
  for (let i = 0; i < tokens.length;) {
    const match = byLength.find((arg) => arg.tokens.every((t, k) => tokens[i + k]?.toLowerCase() === t.toLowerCase()));
    if (match) {
      enabled.add(match.id);
      i += match.tokens.length;
    } else {
      extra.push(tokens[i]);
      i += 1;
    }
  }
  return { enabled, extra };
}

/** The string passed to the game: enabled options in catalog order, then extras. */
export function buildGameArgs({ enabled, extra }: ParsedGameArgs): string {
  const known = GAME_ARGS.filter((a) => enabled.has(a.id)).flatMap((a) => a.tokens);
  return [...known, ...extra].join(" ");
}

/** Turns one option on or off; turning it on switches off the ones it excludes. */
export function setGameArg(args: string, id: string, on: boolean): string {
  const parsed = parseGameArgs(args);
  const arg = GAME_ARGS.find((a) => a.id === id);
  if (!arg) return args;
  if (on) {
    parsed.enabled.add(id);
    for (const other of arg.excludes ?? []) parsed.enabled.delete(other);
  } else {
    parsed.enabled.delete(id);
  }
  return buildGameArgs(parsed);
}

export function isGameArgOn(args: string, id: string): boolean {
  return parseGameArgs(args).enabled.has(id);
}

/** Drops the arguments that have no switch. */
export function withoutExtraArgs(args: string): string {
  return buildGameArgs({ ...parseGameArgs(args), extra: [] });
}
