import { expect, type APIRequestContext, type Page } from "@playwright/test";
import { appendFileSync, existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import http from "node:http";
import { join } from "node:path";
import { E2E } from "../playwright.config";
import { SECRET_FILES, snapshotRealSecrets } from "./real-secrets.mjs";

/** Texts from e2e/fixtures/projects/-tmp-e2e-demo/e2e-session.jsonl. */
export const FIXTURE_FIRST_MESSAGE = "hello from the e2e fixture";
export const FIXTURE_REPLY = "hi from the e2e backend";
export const SCROLL_FIXTURE_FIRST_MESSAGE = "scroll fixture 0";

/** Shape of `clients::Client` as the API serialises it. */
export interface ApiClient {
  id: string;
  name: string;
  builtin: boolean;
  created_at: number;
  issued_at: number;
  revoked_at?: number;
}

export function sessionFile(projectsDir: string): string {
  return join(projectsDir, "-tmp-e2e-demo", "e2e-session.jsonl");
}

/** The credential the backend wrote for a built-in client (`web-ui`, `tui`)
 * in this scratch config dir. */
export function readCredential(configDir: string, client: "web-ui" | "tui"): string {
  return readFileSync(join(configDir, "clients", `${client}.jwt`), "utf8").trim();
}

/** Assert every secret for `configDir` lives on the test path with owner-only
 * permissions, and that the developer's real secrets (the config dir used
 * when `CCTRACE_CONFIG_DIR` is unset) are exactly as `e2e/prepare.mjs` found
 * them before any server started: none created, modified, or deleted. */
export function expectSecretOnTestPath(configDir: string): void {
  for (const file of SECRET_FILES) {
    const testPath = join(configDir, file);
    expect(testPath.startsWith(E2E.tmp)).toBe(true);
    expect(existsSync(testPath), `${testPath} missing`).toBe(true);
    if (process.platform !== "win32") {
      expect(statSync(testPath).mode & 0o777, `${testPath} mode`).toBe(0o600);
    }
  }
  const before = JSON.parse(readFileSync(join(E2E.tmp, "real-secrets.json"), "utf8")) as ReturnType<
    typeof snapshotRealSecrets
  >;
  expect(snapshotRealSecrets(), "real config dir secrets changed during the e2e run").toEqual(
    before,
  );
}

/** Same credential, last signature character flipped: well-formed, wrong MAC. */
export function forge(credential: string): string {
  const last = credential.at(-1);
  return `${credential.slice(0, -1)}${last === "A" ? "B" : "A"}`;
}

/** `GET /api/whoami` as the holder of `credential`. */
export async function whoami(
  request: APIRequestContext,
  apiBase: string,
  credential: string,
): Promise<{ status: number; name: string | null }> {
  const res = await request.get(`${apiBase}/api/whoami`, {
    headers: { "X-CCTrace-Token": credential },
  });
  if (res.status() !== 200) return { status: res.status(), name: null };
  const body = (await res.json()) as { client: { name: string } | null };
  return { status: 200, name: body.client?.name ?? null };
}

/** `GET /api/clients` as the holder of `credential`. */
export async function listClients(
  request: APIRequestContext,
  apiBase: string,
  credential: string,
): Promise<ApiClient[]> {
  const res = await request.get(`${apiBase}/api/clients`, {
    headers: { "X-CCTrace-Token": credential },
  });
  expect(res.status()).toBe(200);
  return (await res.json()) as ApiClient[];
}

/** `POST /api/clients/{id}/reissue` for the client called `name`, acting as the
 * holder of `credential` — what "another client" (a second cctrace process, a
 * script, the desktop app) does. Returns the fresh credential. */
export async function reissueViaApi(
  request: APIRequestContext,
  apiBase: string,
  credential: string,
  name: string,
): Promise<string> {
  const target = (await listClients(request, apiBase, credential)).find((c) => c.name === name);
  expect(target, `client ${name} registered`).toBeDefined();
  const res = await request.post(`${apiBase}/api/clients/${target!.id}/reissue`, {
    headers: { "X-CCTrace-Token": credential },
  });
  expect(res.status()).toBe(200);
  return ((await res.json()) as { credential: string }).credential;
}

/** Append a user turn to the fixture transcript, as Claude Code would while a
 * session is live. The file watcher picks it up and pushes an SSE update. */
export function appendUserMessage(projectsDir: string, text: string): void {
  const seq = Date.now();
  const entry = {
    type: "user",
    uuid: `u-${seq}`,
    parentUuid: "u2",
    sessionId: "e2e-session",
    cwd: "/tmp/e2e-demo",
    timestamp: new Date().toISOString(),
    message: { role: "user", content: text },
  };
  appendFileSync(sessionFile(projectsDir), `${JSON.stringify(entry)}\n`);
}

/** Write an isolated, linked conversation for layout/scrolling tests. Each
 * message points to the previous one so the parser keeps the entire chain. */
export function writeScrollableSession(projectsDir: string, count: number): void {
  let parentUuid: string | null = null;
  const entries: string[] = [];
  for (let index = 0; index < count; index += 1) {
    const uuid = `scroll-${index}`;
    entries.push(
      JSON.stringify({
        type: "user",
        uuid,
        parentUuid,
        sessionId: "scroll-session",
        cwd: "/tmp/e2e-demo",
        timestamp: new Date(Date.UTC(2026, 0, 1, 12, 0, 3 + index)).toISOString(),
        message: {
          role: "user",
          content: `scroll fixture ${index}: ${"enough text to make this row measurable ".repeat(4)}`,
        },
      }),
    );
    parentUuid = uuid;
  }
  writeFileSync(
    join(projectsDir, "-tmp-e2e-demo", "scroll-session.jsonl"),
    `${entries.join("\n")}\n`,
  );
}

/** Open Settings from the toolbar and wait for the Accepted clients table. */
export async function openSettings(page: Page): Promise<void> {
  await page.getByTitle("Settings").click();
  await expect(page.getByRole("table", { name: "Accepted clients" })).toBeVisible();
}

/** With Settings open: register `name` and return the credential shown once. */
export async function addClient(page: Page, name: string): Promise<string> {
  await page.getByLabel("New client name").fill(name);
  await page.getByRole("button", { name: "Add client" }).click();
  await expect(page.getByText(`Client "${name}" registered`)).toBeVisible();
  return readShownCredential(page);
}

/** With Settings open: Reissue `name` (two-click confirm — the button keeps
 * its accessible name and only its label changes while armed). Returns the
 * new credential as shown in the UI. */
export async function reissueClient(page: Page, name: string): Promise<string> {
  const button = page.getByRole("button", { name: `Reissue ${name}`, exact: true });
  await button.click();
  await expect(button).toHaveText("Confirm reissue?");
  await button.click();
  await expect(page.getByText(`"${name}" reissued`)).toBeVisible();
  return readShownCredential(page);
}

/** With Settings open: Revoke `name` (two-click confirm). */
export async function revokeClient(page: Page, name: string): Promise<void> {
  const button = page.getByRole("button", { name: `Revoke ${name}`, exact: true });
  await button.click();
  await expect(button).toHaveText("Confirm revoke?");
  await button.click();
  await expect(page.getByText(`"${name}" revoked`)).toBeVisible();
}

/** Make sure `web-ui` is active, reissuing it as `tui` if an earlier (failed)
 * run left it revoked — so a retry never starts locked out. */
export async function ensureWebUiActive(
  request: APIRequestContext,
  apiBase: string,
  configDir: string,
): Promise<void> {
  const tui = readCredential(configDir, "tui");
  const webUi = (await listClients(request, apiBase, tui)).find((c) => c.name === "web-ui");
  if (webUi?.revoked_at != null) await reissueViaApi(request, apiBase, tui, "web-ui");
}

async function readShownCredential(page: Page): Promise<string> {
  const value = await page.getByLabel("New client credential").inputValue();
  expect(value).toMatch(/^eyJ[\w-]+\.[\w-]+\.[\w-]+$/);
  return value;
}

/** The `cctrace-api-token` Vite plugin serves the `web-ui` credential as a
 * virtual module and invalidates it when the file changes. The file watcher
 * has latency, so before asserting on a cold load, wait until the dev server
 * serves the reissued value. */
export function waitForInjectedToken(
  baseURL: string,
  credential: string,
  timeoutMs = 30_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolvePromise, reject) => {
    let last = "";
    const retry = () => {
      if (Date.now() >= deadline) {
        reject(new Error(`dev server never served the reissued credential (${last})`));
        return;
      }
      setTimeout(attempt, 250);
    };
    const attempt = () => {
      fetch(`${baseURL}/@id/__x00__virtual:cctrace-api-token`)
        .then(async (res) => {
          const body = await res.text();
          if (body.includes(credential)) {
            resolvePromise();
            return;
          }
          last = `status ${res.status}, credential not in module yet`;
          retry();
        })
        .catch((err: unknown) => {
          last = String(err);
          retry();
        });
    };
    attempt();
  });
}

/** Raw HTTP GET with full control over the Host header (browsers and
 * Playwright's request fixture won't let us spoof it). */
export function rawGet(
  port: number,
  path: string,
  headers: Record<string, string> = {},
): Promise<{ status: number; headers: http.IncomingHttpHeaders }> {
  return new Promise((resolvePromise, reject) => {
    const req = http.request({ host: "127.0.0.1", port, path, method: "GET", headers }, (res) => {
      res.resume();
      res.on("end", () => resolvePromise({ status: res.statusCode ?? 0, headers: res.headers }));
    });
    req.on("error", reject);
    req.end();
  });
}
