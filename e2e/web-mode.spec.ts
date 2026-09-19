/**
 * `cctrace --web` shape: the Vite dev server serves the UI on one origin and
 * the backend answers on another. No cookie can help here — the Vite plugin
 * reads the built-in `web-ui` client's credential file the backend wrote and
 * injects it, and the UI sends it as a header on fetch calls and as a query
 * parameter on the EventSource stream.
 */
import { expect, test } from "@playwright/test";
import { E2E } from "../playwright.config";
import {
  expectSecretOnTestPath,
  FIXTURE_FIRST_MESSAGE,
  openSettings,
  readCredential,
  reissueClient,
  reissueViaApi,
  waitForInjectedToken,
  whoami,
} from "./helpers";

const { apiPort, configDir } = E2E.webMode;
const api = `http://127.0.0.1:${apiPort}`;

test("Vite plugin serves the web-ui credential, sent as a header", async ({ page, request }) => {
  const firstApiCall = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/`) && !r.url().includes("/api/events"),
  );
  await page.goto("/");
  const req = await firstApiCall;
  const credential = readCredential(configDir, "web-ui");
  expect(req.headers()["x-cctrace-token"]).toBe(credential);
  expect(await whoami(request, api, credential)).toEqual({ status: 200, name: "web-ui" });
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
});

test("opens a cached efficiency dashboard from the picker without opening the session", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();

  await page.getByRole("button", { name: "Dashboard · 82" }).click();

  await expect(page.getByRole("dialog", { name: "Efficiency Dashboard" })).toBeVisible();
  await expect(page.getByText("SESSION EFFICIENCY")).toBeVisible();
  await Promise.all(
    [
      "Progress",
      "Useful tool calls",
      "Avoided repeated work",
      "Proportionate exploration",
      "Avoided thrashing",
      "Effective recovery",
      "Efficient token use",
      "Useful subagents",
      "Task completion",
    ].map((metric) => expect(page.getByText(metric, { exact: true })).toBeVisible()),
  );
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Detail" })).toBeVisible();
});

test("SSE stream carries the credential as a query parameter", async ({ page }) => {
  const sse = page.waitForRequest((r) => r.url().startsWith(`${api}/api/events`));
  await page.goto("/");
  const url = new URL((await sse).url());
  expect(url.searchParams.get("token")).toBe(readCredential(configDir, "web-ui"));
});

test("Reissuing web-ui hot-swaps this tab's credential — no reload — and SSE + fetch follow", async ({
  page,
  baseURL,
  request,
}) => {
  await page.goto("/");
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
  const oldCredential = readCredential(configDir, "web-ui");
  let loads = 0;
  page.on("load", () => loads++);

  const reconnected = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/events`) && !r.url().includes(oldCredential),
  );
  await openSettings(page);
  const newCredential = await reissueClient(page, "web-ui");
  expect(newCredential).not.toBe(oldCredential);
  expect(readCredential(configDir, "web-ui")).toBe(newCredential);
  expectSecretOnTestPath(configDir);
  expect((await whoami(request, api, oldCredential)).status).toBe(401);

  // listen.ts reopens the stream with the reissued credential, without a reload.
  expect(new URL((await reconnected).url()).searchParams.get("token")).toBe(newCredential);

  // The plugin hot-swaps the virtual module over HMR when the file changes.
  // Wait for the dev server to serve the new value, then prove the page was
  // NOT reloaded meanwhile: the Settings modal the user clicked in is still
  // open, showing the notice. (The dev server used to restart here, and
  // Vite's client full-reloaded the page — exactly what CI caught.)
  await waitForInjectedToken(baseURL!, newCredential);
  expect(loads).toBe(0);
  await expect(page.getByText(/"web-ui" reissued/)).toBeVisible();
  await expect(page.getByLabel("New client credential")).toHaveValue(newCredential);

  // A cold load authenticates with the reissued credential straight away.
  const nextFetch = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/`) && !r.url().includes("/api/events"),
  );
  await page.goto("/");
  expect((await nextFetch).headers()["x-cctrace-token"]).toBe(newCredential);
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
});

test("a reissue by another client reaches an open tab over HMR", async ({
  page,
  baseURL,
  request,
}) => {
  await page.goto("/");
  await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
  const oldCredential = readCredential(configDir, "web-ui");
  let loads = 0;
  page.on("load", () => loads++);

  // Reissue from outside the browser, the way the desktop app, the TUI or a
  // script would: call the API as the `tui` client. The backend rewrites
  // `clients/web-ui.jwt`; the dev server pushes it to this tab; the tab's SSE
  // stream must come back on the new credential.
  const reconnected = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/events`) && !r.url().includes(oldCredential),
  );
  const reissued = await reissueViaApi(request, api, readCredential(configDir, "tui"), "web-ui");
  expect(readCredential(configDir, "web-ui")).toBe(reissued);

  expect(new URL((await reconnected).url()).searchParams.get("token")).toBe(reissued);
  await waitForInjectedToken(baseURL!, reissued);
  expect(loads).toBe(0);

  // …and the API accepts the tab's next call with the reissued credential.
  const nextFetch = page.waitForRequest(
    (r) => r.url().startsWith(`${api}/api/`) && !r.url().includes("/api/events"),
  );
  await page.getByTitle("Settings").click();
  const req = await nextFetch;
  expect(req.headers()["x-cctrace-token"]).toBe(reissued);
  expect((await req.response())?.status()).toBe(200);
});
