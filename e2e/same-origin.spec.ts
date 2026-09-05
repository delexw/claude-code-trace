/**
 * Docker shape: one backend serves `/api/*` and the built UI from the same
 * origin. The browser never learns the token — the server hands it over as an
 * `HttpOnly` cookie on the HTML shell, gated on an allowlisted `Host`.
 */
import { expect, test } from "@playwright/test";
import { E2E } from "../playwright.config";
import {
  appendUserMessage,
  FIXTURE_FIRST_MESSAGE,
  FIXTURE_REPLY,
  rawGet,
  readToken,
  regenerateToken,
  openSettings,
} from "./helpers";

const { port, configDir, projectsDir } = E2E.sameOrigin;

test.describe("HTTP API without a browser", () => {
  test("refuses anonymous callers and accepts the shared token", async ({ request }) => {
    const token = readToken(configDir);

    const anonymous = await request.get("/api/settings");
    expect(anonymous.status()).toBe(401);
    expect((await anonymous.json()).error).toContain("API token");

    const viaHeader = await request.get("/api/settings", {
      headers: { "X-CCTrace-Token": token },
    });
    expect(viaHeader.status()).toBe(200);
    const settings = await viaHeader.json();
    expect(settings.api_auth_enabled).toBe(true);
    expect(settings.api_token_source).toBe("file");
    expect(settings.api_token).toBe(token);

    const viaBearer = await request.get("/api/project-dirs", {
      headers: { Authorization: `Bearer ${token}` },
    });
    expect(viaBearer.status()).toBe(200);

    const wrong = await request.get("/api/settings", {
      headers: { "X-CCTrace-Token": `${token.slice(0, -1)}0` },
    });
    expect(wrong.status()).toBe(401);
  });

  test("static shell only hands out the token cookie to an allowlisted Host", async () => {
    const spoofed = await rawGet(port, "/", { Host: "attacker.example:1421" });
    expect(spoofed.status).toBe(200);
    expect(spoofed.headers["set-cookie"]).toBeUndefined();

    const loopback = await rawGet(port, "/", { Host: `localhost:${port}` });
    expect(loopback.status).toBe(200);
    const cookie = (loopback.headers["set-cookie"] ?? []).join(";");
    expect(cookie).toContain(`cctrace_token=${readToken(configDir)}`);
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Strict");
  });
});

test.describe("browser UI", () => {
  test("authenticates via the server-set cookie and shows the session", async ({
    page,
    context,
    baseURL,
  }) => {
    await page.goto("/");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();

    const cookie = (await context.cookies(baseURL!)).find((c) => c.name === "cctrace_token");
    expect(cookie?.value).toBe(readToken(configDir));
    expect(cookie?.httpOnly).toBe(true);
    expect(cookie?.sameSite).toBe("Strict");

    await page.getByText(FIXTURE_FIRST_MESSAGE).click();
    await expect(page.getByText(FIXTURE_REPLY)).toBeVisible();
  });

  test("live-tails the session over the cookie-authenticated SSE stream", async ({ page }) => {
    await page.goto("/");
    await page.getByText(FIXTURE_FIRST_MESSAGE).click();
    await expect(page.getByText(FIXTURE_REPLY)).toBeVisible();

    const live = `appended live ${Date.now()}`;
    appendUserMessage(projectsDir, live);
    await expect(page.getByText(live)).toBeVisible({ timeout: 15_000 });
  });

  test("shows a banner instead of the picker when the client is not accepted", async ({
    page,
    context,
  }) => {
    // Simulate a client the server refuses to hand the cookie to (e.g. a page
    // reached via an unallowlisted host): serve the real HTML shell but from a
    // Node-side fetch, so the browser never sees the `Set-Cookie` header and
    // has no token for the API calls that follow. (`route.fetch()` would go
    // through the browser's cookie jar and store the cookie anyway.)
    await context.clearCookies();
    await page.route("**/", async (route) => {
      const body = await (await fetch(route.request().url())).text();
      await route.fulfill({ status: 200, contentType: "text/html", body });
    });
    await page.goto("/");
    await expect(page.getByRole("alert")).toContainText("Not an accepted client");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toHaveCount(0);
  });

  test("Settings shows the token and Regenerate rotates it without breaking the tab", async ({
    page,
    request,
  }) => {
    const oldToken = readToken(configDir);
    await page.goto("/");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();

    await openSettings(page);
    const field = page.getByLabel("API token");
    await expect(field).toHaveValue(oldToken);
    await expect(field).toHaveAttribute("type", "password");
    await page.getByRole("button", { name: "Show" }).click();
    await expect(field).toHaveAttribute("type", "text");

    const newToken = await regenerateToken(page);
    await expect(field).toHaveValue(newToken);
    expect(newToken).not.toBe(oldToken);
    expect(readToken(configDir)).toBe(newToken);

    // The old token is dead for everyone…
    const stale = await request.get("/api/settings", { headers: { "X-CCTrace-Token": oldToken } });
    expect(stale.status()).toBe(401);
    // …and this tab was re-issued the cookie, so a reload still works.
    await page.reload();
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
  });
});
