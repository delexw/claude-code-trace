/**
 * The shared HTTP API client token, as seen by the browser frontend.
 *
 * The Rust backend gates every `/api/*` route behind a secret token (see
 * `src-tauri/src/auth.rs`). How this module learns it depends on the mode:
 *
 * - `cctrace --web` (Vite on 1420, API on 11423): the Vite plugin in
 *   `vite.config.ts` reads the token file and injects it as
 *   `import.meta.env.VITE_API_TOKEN` at serve time. `invoke.ts` sends it as an
 *   `X-CCTrace-Token` header; `listen.ts` appends it as `?token=` because
 *   `EventSource` cannot set headers.
 * - Docker (same-origin bundle): this stays empty. The server sets an HttpOnly
 *   cookie on the HTML shell, which the browser attaches automatically.
 * - Tauri desktop: never used — the webview talks over IPC, not HTTP.
 *
 * Kept separate from `invoke.ts` / `listen.ts` so neither imports the other.
 */

let token: string | null = (import.meta.env.VITE_API_TOKEN as string | undefined) || null;

/** The token currently sent with HTTP API calls, or null when none is set. */
export function getApiToken(): string | null {
  return token;
}

/** Swap the live token — used after Settings → Regenerate so the current tab
 * keeps working without a reload. */
export function setApiToken(next: string | null | undefined): void {
  token = next || null;
}

/** Headers to spread into a `fetch` call to the HTTP API. */
export function authHeaders(): Record<string, string> {
  return token ? { "X-CCTrace-Token": token } : {};
}

/** Append `token=` to a URL (for `EventSource`, which cannot set headers). */
export function withTokenQuery(url: string): string {
  if (!token) return url;
  const sep = url.includes("?") ? "&" : "?";
  return `${url}${sep}token=${encodeURIComponent(token)}`;
}
