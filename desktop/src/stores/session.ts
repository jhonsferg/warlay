import { defineStore } from "pinia";
import { ref } from "vue";
import { auth } from "../services/api";
import { googleFailureMessage } from "../services/googleSignIn";
import { native } from "../services/native";
import { ApiError, hasSession, onSessionChange, setTokens } from "../services/http";
import { isServerUnavailable, RESTORE_RETRY_MS } from "../services/outage";
import type { Me } from "../services/types";

export type AuthStatus = "unknown" | "signedOut" | "signedIn";

/** The signed-in account. Other stores start/stop around its status. */
export const useSession = defineStore("session", () => {
  const me = ref<Me | null>(null);
  const status = ref<AuthStatus>("unknown");

  onSessionChange((signedIn) => {
    if (!signedIn) {
      me.value = null;
      status.value = "signedOut";
    }
  });

  async function restore() {
    if (!hasSession()) {
      status.value = "signedOut";
      return;
    }
    for (let attempt = 0; ; attempt++) {
      try {
        me.value = await auth.me();
        status.value = "signedIn";
        return;
      } catch (e) {
        // Only the server refusing the session ends it. A server that is down or
        // being replaced is waited for a few seconds; if it stays away the
        // session is kept for the next start, not erased.
        if (e instanceof ApiError && !isServerUnavailable(e.status)) {
          setTokens(null);
          status.value = "signedOut";
          return;
        }
        if (attempt >= RESTORE_RETRY_MS.length) {
          status.value = "signedOut";
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, RESTORE_RETRY_MS[attempt]));
      }
    }
  }

  async function login(login: string, password: string, totpCode?: string) {
    const res = await auth.login(login, password, totpCode);
    setTokens(res.tokens);
    me.value = res.user;
    status.value = "signedIn";
  }

  /**
   * Signs in with Google, first half: the system browser does the sign-in
   * while the app waits, and returns a one-time grant. Throws an Error with a
   * message fit for the player.
   */
  async function googleBrowser(webUrl: string) {
    let grant;
    try {
      grant = await native.googleSignIn(webUrl, navigator.language);
    } catch (e) {
      throw new Error(googleFailureMessage(e));
    }
    if (!grant) throw new Error(googleFailureMessage("desktop_only"));
    return grant;
  }

  /**
   * Second half: the server turns the grant into a session. It answers
   * `totp_required` (an ApiError) for accounts with an authenticator app; call
   * again with the same grant and the code.
   */
  async function googleExchange(grant: { code: string; verifier: string }, totpCode?: string) {
    const res = await auth.googleExchange(grant.code, grant.verifier, totpCode);
    setTokens(res.tokens);
    me.value = res.user;
    status.value = "signedIn";
  }

  /** Creates the account and signs in. Returns the recovery codes, to show once. */
  async function register(body: { username: string; email: string; password: string; displayName?: string }) {
    const res = await auth.register(body);
    setTokens(res.tokens);
    me.value = res.user;
    status.value = "signedIn";
    return res.recoveryCodes ?? [];
  }

  async function logout() {
    try {
      await auth.logout();
    } catch {
      /* the session ends locally regardless */
    }
    setTokens(null);
  }

  async function update(body: { displayName?: string; bio?: string; avatarColor?: string }) {
    me.value = await auth.updateMe(body);
  }

  return { me, status, restore, login, googleBrowser, googleExchange, register, logout, update };
});
