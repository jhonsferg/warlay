/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** The Warlay server's address, injected when the app is built (GitHub
   * secret in CI, environment variable or `desktop/.env` locally). Required
   * for production builds; see `services/serverUrl.ts`. */
  readonly VITE_WARLAY_MAIN_URL?: string;
}
