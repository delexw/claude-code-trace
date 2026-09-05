/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Shared HTTP API client token, injected by the Vite plugin in
   * `vite.config.ts` in dev/web mode only. Empty in production bundles. */
  readonly VITE_API_TOKEN?: string;
  readonly VITE_API_BASE?: string;
}
