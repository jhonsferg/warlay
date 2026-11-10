// The app's two languages. Spanish is the language of the source: every
// user-facing text is written in Spanish inside `t("...")`, and English is a
// dictionary keyed by that Spanish text (see `../i18n/en.ts`). A text with no
// English entry is shown in Spanish, so a gap never shows a key or breaks a
// screen; `i18n.test.ts` fails when one exists.
import { ref } from "vue";
import { EN } from "../i18n/en.ts";
import { EN_SERVER } from "../i18n/enServer.ts";

export type Locale = "es" | "en";

export const LOCALES: { id: Locale; label: string }[] = [
  { id: "es", label: "Español" },
  { id: "en", label: "English" },
];

const STORAGE_KEY = "warlay.locale";

/** The browser's storage, when there is one (plain Node tests have none). */
const storage = typeof localStorage === "undefined" ? null : localStorage;

/** Spanish unless the system is set to English; a saved choice wins over both. */
export function initialLocale(saved: string | null, systemLanguages: readonly string[]): Locale {
  if (saved === "es" || saved === "en") return saved;
  const first = systemLanguages.map((l) => l.toLowerCase()).find((l) => l.startsWith("es") || l.startsWith("en"));
  return first?.startsWith("en") ? "en" : "es";
}

export const locale = ref<Locale>(
  initialLocale(
    storage?.getItem(STORAGE_KEY) ?? null,
    // Only a real window has a language to ask for (Node's `navigator` is the
    // machine running the tests, not a player).
    typeof window === "undefined" ? [] : (navigator.languages ?? [navigator.language]),
  ),
);

function applyToDocument(next: Locale) {
  if (typeof document !== "undefined") document.documentElement.lang = next;
}
applyToDocument(locale.value);

export function setLocale(next: Locale) {
  locale.value = next;
  storage?.setItem(STORAGE_KEY, next);
  applyToDocument(next);
}

/** Replaces `{name}` placeholders. */
export function fill(template: string, params?: Record<string, string | number>): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => (key in params ? String(params[key]) : whole));
}

/**
 * The text in the current language. `text` is the Spanish original, with
 * `{name}` placeholders for the parts that vary. Reading `locale` here makes
 * every template that calls it redraw when the language changes.
 */
export function t(text: string, params?: Record<string, string | number>): string {
  const template = locale.value === "en" ? (EN[text] ?? text) : text;
  return fill(template, params);
}

/** A message the server wrote in an error (always Spanish there), in the current language. */
export function tServer(message: string): string {
  return locale.value === "en" ? (EN_SERVER[message] ?? message) : message;
}

/**
 * Marks a Spanish text that is kept in a table or constant and shown later:
 * it returns the text as it is, so the dictionary and the coverage test see it,
 * and whoever shows it calls `t(text)` at that moment (a constant is built once,
 * before the language is known, and would not follow a change of language).
 */
export function tx(text: string): string {
  return text;
}

/** The language tag for `Intl` and `toLocale...String`. */
export function localeTag(): string {
  return locale.value === "en" ? "en" : "es";
}
