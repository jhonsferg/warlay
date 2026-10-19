/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Optional build-time override of the main server URL; the production URL
   * itself lives in `services/http.ts`. */
  readonly VITE_WARLAY_MAIN_URL?: string;
}
