/**
 * Playwright end-to-end suite.
 *
 * Boots the real headless Rust backend and drives the real web UI in Chromium
 * in the two shapes the HTTP API's client verification has to work in:
 *
 * - **same-origin** — the Docker shape. One backend serves `/api/*` and the
 *   built frontend bundle from the same port; the browser authenticates with
 *   the `HttpOnly` cookie the server sets on the HTML shell.
 * - **web-mode** — the `cctrace --web` shape. The Vite dev server on one port
 *   talks cross-origin to a backend on another; the Vite plugin injects the
 *   shared token, which travels as a header (fetch) and a query param (SSE).
 *
 * Everything runs against `e2e/.tmp` (`CCTRACE_CONFIG_DIR`, `CLAUDE_PROJECTS_DIR`)
 * so a run never touches a developer's real token or sessions.
 */
import { defineConfig, devices } from "@playwright/test";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const tmp = resolve(root, "e2e/.tmp");
const binary = resolve(root, "src-tauri/target/debug/claude-code-trace");

/** Ports and paths shared with the specs (see e2e/helpers.ts). */
export const E2E = {
  tmp,
  sameOrigin: {
    port: 11431,
    configDir: resolve(tmp, "same/config"),
    projectsDir: resolve(tmp, "same/projects"),
  },
  webMode: {
    apiPort: 11432,
    uiPort: 14200,
    configDir: resolve(tmp, "web/config"),
    projectsDir: resolve(tmp, "web/projects"),
  },
} as const;

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: "./e2e",
  testMatch: /.*\.spec\.ts$/,
  // Regenerate tests rotate shared server state, so run serially.
  fullyParallel: false,
  workers: 1,
  retries: isCI ? 1 : 0,
  forbidOnly: isCI,
  reporter: isCI ? [["github"], ["html", { open: "never" }]] : [["list"]],
  timeout: 30_000,
  expect: { timeout: 10_000 },
  use: {
    ...devices["Desktop Chrome"],
    trace: "on-first-retry",
  },
  projects: [
    {
      name: "same-origin",
      testMatch: /same-origin\.spec\.ts$/,
      use: { baseURL: `http://127.0.0.1:${E2E.sameOrigin.port}` },
    },
    {
      name: "web-mode",
      testMatch: /web-mode\.spec\.ts$/,
      use: { baseURL: `http://localhost:${E2E.webMode.uiPort}` },
    },
  ],
  // Started in array order. The first entry runs e2e/prepare.mjs (scratch
  // dirs, headless cargo build, frontend bundle) before its server, so the
  // later entries can rely on the binary and bundle existing.
  webServer: [
    {
      command: `node e2e/prepare.mjs && "${binary}" --headless`,
      // A 401 counts as "up": the probe carries no token on purpose.
      url: `http://127.0.0.1:${E2E.sameOrigin.port}/api/settings`,
      timeout: 15 * 60_000,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        CCTRACE_CONFIG_DIR: E2E.sameOrigin.configDir,
        CLAUDE_PROJECTS_DIR: E2E.sameOrigin.projectsDir,
        CCTRACE_HTTP_PORT: String(E2E.sameOrigin.port),
        CCTRACE_STATIC_DIR: resolve(tmp, "dist"),
      },
    },
    {
      command: `"${binary}" --headless`,
      url: `http://127.0.0.1:${E2E.webMode.apiPort}/api/settings`,
      timeout: 60_000,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        CCTRACE_CONFIG_DIR: E2E.webMode.configDir,
        CLAUDE_PROJECTS_DIR: E2E.webMode.projectsDir,
        CCTRACE_HTTP_PORT: String(E2E.webMode.apiPort),
        CCTRACE_ALLOWED_ORIGINS: `http://localhost:${E2E.webMode.uiPort}`,
      },
    },
    {
      command: `npx vite --port ${E2E.webMode.uiPort} --strictPort`,
      url: `http://localhost:${E2E.webMode.uiPort}/`,
      timeout: 120_000,
      reuseExistingServer: false,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        CCTRACE_CONFIG_DIR: E2E.webMode.configDir,
        VITE_API_BASE: `http://127.0.0.1:${E2E.webMode.apiPort}`,
      },
    },
  ],
});
