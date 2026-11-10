// What the player is told when signing in with Google does not complete. The
// shell and the website report short codes (see commands/google.rs and the
// server's oauth module); this turns them into sentences.

const MESSAGES: Record<string, string> = {
  cancelled: "Cancelaste el inicio de sesión con Google.",
  timeout: "El inicio de sesión con Google tardó demasiado. Inténtalo de nuevo.",
  access_denied: "Cancelaste el inicio de sesión con Google.",
  email_not_verified: "Google no ha verificado el correo de esa cuenta, así que no se puede usar para entrar.",
  account_conflict: "Ya existe una cuenta con ese correo que no tiene contraseña. Entra con la cuenta original.",
  provider_unreachable: "No se pudo contactar con Google. Revisa tu conexión e inténtalo de nuevo.",
  provider_refused: "Google rechazó el inicio de sesión. Inténtalo de nuevo.",
  provider_invalid: "Google respondió algo inesperado. Inténtalo de nuevo.",
  desktop_only: "El inicio de sesión con Google solo está disponible en la aplicación de escritorio.",
  open_failed: "No se pudo abrir el navegador.",
  unavailable: "El servidor no está disponible ahora. Inténtalo en unos minutos.",
  server_error: "No se pudo completar el inicio de sesión. Inténtalo de nuevo.",
};

const FALLBACK = "No se pudo iniciar sesión con Google. Inténtalo de nuevo.";

/** The message for a failure, which may be a code, an Error or text with a code in front. */
export function googleFailureMessage(failure: unknown): string {
  const text = failure instanceof Error ? failure.message : String(failure ?? "");
  // `open_failed: <os detail>` and the like: the code is what comes before the colon.
  const code = text.split(":")[0]?.trim() ?? "";
  return MESSAGES[code] ?? FALLBACK;
}

/** The player gave up on their own: not worth an error message. */
export function isGoogleCancel(failure: unknown): boolean {
  const text = failure instanceof Error ? failure.message : String(failure ?? "");
  return text === MESSAGES.cancelled || text === MESSAGES.access_denied;
}
