/** Type declarations for `api-token.mjs` so `vite.config.ts` can import it under `strict`. */
export interface ApiTokenOptions {
  platform?: NodeJS.Platform;
  env?: NodeJS.ProcessEnv;
  home?: string;
}
export function configDir(opts?: ApiTokenOptions): string;
export function apiTokenPath(opts?: ApiTokenOptions): string;
export function resolveApiToken(opts?: ApiTokenOptions): string | null;
