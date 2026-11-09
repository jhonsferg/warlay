// Helpers for the account recovery codes (see crates/social/src/recovery.rs).

/** The text a player can paste into a notes app or print. */
export function codesText(username: string, codes: string[]): string {
  return [
    `Warlay - recovery codes for @${username}`,
    "",
    "Each code works once. With one of them you can set a new password",
    "without e-mail. Keep them somewhere safe and private.",
    "",
    ...codes.map((code, i) => `${String(i + 1).padStart(2, " ")}. ${code}`),
    "",
  ].join("\n");
}

/**
 * Whether `input` can be a recovery code, tolerating what a person types: any
 * case, with or without the dash or spaces, and `O`, `I`, `L` for the digits
 * they look like. The server normalizes the same way and does the real check.
 */
export function looksLikeCode(input: string): boolean {
  const compact = input.replace(/[\s-]/g, "").toUpperCase();
  return /^[0-9A-HJKMNP-TV-ZOIL]{10}$/.test(compact);
}
