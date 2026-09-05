#!/usr/bin/env node
/**
 * One-shot setup for the Playwright suite, run by the first `webServer` entry
 * in playwright.config.ts before the backend starts:
 *
 * 1. Recreate `e2e/.tmp` with an isolated config dir + a copy of the session
 *    fixtures for each deployment shape (tests mutate both).
 * 2. Build the headless backend binary (no Tauri/GTK). Set
 *    `CCTRACE_E2E_SKIP_BUILD=1` to reuse an existing build.
 * 3. Build the frontend bundle the same way the Docker image does
 *    (`VITE_API_BASE=""` → same-origin relative URLs) into `e2e/.tmp/dist`.
 */
import { execSync } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tmp = resolve(root, "e2e/.tmp");
const fixtures = resolve(root, "e2e/fixtures/projects");

rmSync(tmp, { recursive: true, force: true });
for (const shape of ["same", "web"]) {
  mkdirSync(resolve(tmp, shape, "config"), { recursive: true });
  cpSync(fixtures, resolve(tmp, shape, "projects"), { recursive: true });
}

const run = (cmd, env = {}) =>
  execSync(cmd, { cwd: root, stdio: "inherit", env: { ...process.env, ...env } });

if (!process.env.CCTRACE_E2E_SKIP_BUILD) {
  run(
    "cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --bin claude-code-trace",
  );
}
run(`npx vite build --outDir "${resolve(tmp, "dist")}" --emptyOutDir`, { VITE_API_BASE: "" });

console.log(`e2e: scratch space ready at ${tmp}`);
