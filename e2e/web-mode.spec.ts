/**
 * `cctrace --web` shape: the Vite dev server serves the UI on one origin and
 * the backend answers on another. No cookie can help here — the Vite plugin
 * reads the shared token file and injects it, and the UI sends it as a header
 * on fetch calls and as a query parameter on the EventSource stream.
 */
import { expect, test } from "@playwright/test";
import { E2E } from "../playwright.config";
import {
  FIXTURE_FIRST_MESSAGE,
  openSettings,
  readToken,
  regenerateToken,
  waitForInjectedToken,
} from "./helpers";

const { apiPort, configDir } = E2E.webMode;
const api = `http://127.0.0.1:${apiPort}`;

test("Vite plugin and backend converged on one token, sent as a header", async ({ page }) => {
  const firstApiCall = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/`) && !r.url().includes("/api/events"),
  );
  await page.goto("/");
  const req = await firstApiCall;
  expect(req.headers()["x-cctrace-token"]).toBe(readToken(configDir));
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
});

test("SSE stream carries the token as a query parameter", async ({ page }) => {
  const sse = page.waitForRequest((r) => r.url().startsWith(`${api}/api/events`));
  await page.goto("/");
  const url = new URL((await sse).url());
  expect(url.searchParams.get("token")).toBe(readToken(configDir));
});

test("Regenerate switches this tab to the new token on fetch and SSE", async ({
  page,
  baseURL,
}) => {
  await page.goto("/");
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
  const oldToken = readToken(configDir);

  const reconnected = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/events`) && !r.url().includes(oldToken),
  );
  await openSettings(page);
  const newToken = await regenerateToken(page);
  expect(newToken).not.toBe(oldToken);
  expect(readToken(configDir)).toBe(newToken);

  // listen.ts reopens the stream with the rotated token without a reload.
  expect(new URL((await reconnected).url()).searchParams.get("token")).toBe(newToken);

  // The Vite plugin watches the token file and restarts the dev server so a
  // fresh page load gets the new VITE_API_TOKEN baked in. Wait for that to
  // land, then prove a cold load authenticates with the rotated token.
  await waitForInjectedToken(baseURL!, newToken);
  const nextFetch = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/`) && !r.url().includes("/api/events"),
  );
  await page.goto("/");
  expect((await nextFetch).headers()["x-cctrace-token"]).toBe(newToken);
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
});
