/**
 * Shared HTTP API client token — Node side.
 *
 * The Rust backend (`src-tauri/src/auth.rs`) requires every `/api/*` request
 * to carry a secret token. In `cctrace --web` the browser UI is served by the
 * Vite dev server on a different origin from the API, so it can't receive the
 * token as a same-origin cookie the way the Docker bundle does. Instead the
 * Vite plugin in `vite.config.ts` calls `resolveApiToken()` here and injects
 * the value as `import.meta.env.VITE_API_TOKEN` at *serve* time only.
 *
 * This module must mirror the Rust resolution exactly:
 *   1. `CCTRACE_API_AUTH=off`   → null (verification disabled)
 *   2. `CCTRACE_API_TOKEN=<t>`  → that token
 *   3. `<config dir>/claude-code-trace/api-token` → read it, or create it.
 *
 * Creation uses `wx` (O_EXCL). `tauri dev` starts Vite *before* the Rust
 * binary, so on a first run either side may be the creator; the loser sees
 * EEXIST and re-reads what the winner wrote, so both converge on one token.
 */
import { randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, platform as osPlatform } from "node:os";
import { dirname, join } from "node:path";

/** Mirror of Rust's `dirs::config_dir()` for the three supported platforms. */
export function configDir({ platform = osPlatform(), env = process.env, home = homedir() } = {}) {
  if (platform === "win32") return env.APPDATA || join(home, "AppData", "Roaming");
  if (platform === "darwin") return join(home, "Library", "Application Support");
  return env.XDG_CONFIG_HOME || join(home, ".config");
}

/** `<config dir>/claude-code-trace/api-token` — sibling of `settings.json`. */
export function apiTokenPath(opts = {}) {
  return join(configDir(opts), "claude-code-trace", "api-token");
}

function readToken(path) {
  try {
    const t = readFileSync(path, "utf8").trim();
    return t || null;
  } catch {
    return null;
  }
}

/**
 * Resolve the token the web UI should send, or `null` when verification is
 * off. Creates the token file (mode 0600) when it does not exist yet.
 */
export function resolveApiToken(opts = {}) {
  const env = opts.env ?? process.env;
  if ((env.CCTRACE_API_AUTH ?? "").trim().toLowerCase() === "off") return null;
  const fromEnv = (env.CCTRACE_API_TOKEN ?? "").trim();
  if (fromEnv) return fromEnv;

  const path = apiTokenPath({ ...opts, env });
  const existing = readToken(path);
  if (existing) return existing;

  mkdirSync(dirname(path), { recursive: true });
  const fresh = randomBytes(32).toString("hex");
  try {
    writeFileSync(path, `${fresh}\n`, { flag: "wx", mode: 0o600 });
    return fresh;
  } catch (err) {
    if (err && err.code === "EEXIST") return readToken(path) ?? fresh;
    throw err;
  }
}
