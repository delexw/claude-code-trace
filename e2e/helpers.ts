import { expect, type Page } from "@playwright/test";
import { appendFileSync, readFileSync } from "node:fs";
import http from "node:http";
import { join } from "node:path";

/** Texts from e2e/fixtures/projects/-tmp-e2e-demo/e2e-session.jsonl. */
export const FIXTURE_FIRST_MESSAGE = "hello from the e2e fixture";
export const FIXTURE_REPLY = "hi from the e2e backend";

export function sessionFile(projectsDir: string): string {
  return join(projectsDir, "-tmp-e2e-demo", "e2e-session.jsonl");
}

/** The shared token the backend wrote (or adopted) in this scratch config dir. */
export function readToken(configDir: string): string {
  return readFileSync(join(configDir, "api-token"), "utf8").trim();
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

/** Open Settings from the toolbar and wait for the API access section. */
export async function openSettings(page: Page): Promise<void> {
  await page.getByTitle("Settings").click();
  await expect(page.getByLabel("API token")).toBeVisible();
}

/** With Settings already open, drive API access → Regenerate (two-click
 * confirm). Returns the new token as shown in the UI. */
export async function regenerateToken(page: Page): Promise<string> {
  const field = page.getByLabel("API token");
  await page.getByRole("button", { name: "Regenerate" }).click();
  await page.getByRole("button", { name: "Confirm regenerate?" }).click();
  await expect(page.getByText(/Token regenerated/)).toBeVisible();
  const value = await field.inputValue();
  expect(value).toMatch(/^[0-9a-f]{64}$/);
  return value;
}

/** The Vite dev server restarts itself when the token file changes (see the
 * plugin in vite.config.ts). Polling for "is it up" races that restart, so
 * wait for the proof that it finished: the served `apiToken.ts` module now
 * has the new token baked in. Connection errors during the restart are
 * expected and swallowed. */
export function waitForInjectedToken(
  baseURL: string,
  token: string,
  timeoutMs = 30_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolvePromise, reject) => {
    let last = "";
    const retry = () => {
      if (Date.now() >= deadline) {
        reject(new Error(`dev server never served the rotated token (${last})`));
        return;
      }
      setTimeout(attempt, 250);
    };
    const attempt = () => {
      fetch(`${baseURL}/src/lib/apiToken.ts`)
        .then(async (res) => {
          const body = await res.text();
          if (body.includes(token)) {
            resolvePromise();
            return;
          }
          last = `status ${res.status}, token not in module yet`;
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
