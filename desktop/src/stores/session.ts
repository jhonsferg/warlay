import { defineStore } from "pinia";
import { ref } from "vue";
import { auth } from "../services/api";
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

  async function login(login: string, password: string) {
    const res = await auth.login(login, password);
    setTokens(res.tokens);
    me.value = res.user;
    status.value = "signedIn";
  }

  async function register(body: { username: string; email: string; password: string; displayName?: string }) {
    const res = await auth.register(body);
    setTokens(res.tokens);
    me.value = res.user;
    status.value = "signedIn";
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

  return { me, status, restore, login, register, logout, update };
});
