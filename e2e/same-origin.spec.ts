/**
 * Docker shape: one backend serves `/api/*` and the built UI from the same
 * origin. The browser never learns its credential — the server hands the
 * built-in `web-ui` client's credential over as an `HttpOnly` cookie on the
 * HTML shell, gated on an allowlisted `Host`.
 */
import { expect, test } from "@playwright/test";
import { E2E } from "../playwright.config";
import {
  addClient,
  appendUserMessage,
  ensureWebUiActive,
  expectSecretOnTestPath,
  FIXTURE_FIRST_MESSAGE,
  FIXTURE_REPLY,
  forge,
  listClients,
  openSettings,
  rawGet,
  readCredential,
  reissueClient,
  reissueViaApi,
  revokeClient,
  SCROLL_FIXTURE_FIRST_MESSAGE,
  whoami,
  writeScrollableSession,
} from "./helpers";

const { port, configDir, projectsDir } = E2E.sameOrigin;
const api = `http://127.0.0.1:${port}`;

test.describe("HTTP API without a browser", () => {
  test("keeps every secret on the test path, never the real config dir", () => {
    expectSecretOnTestPath(configDir);
    expectSecretOnTestPath(E2E.webMode.configDir);
  });

  test("refuses anonymous callers and identifies each client by its credential", async ({
    request,
  }) => {
    const tui = readCredential(configDir, "tui");
    const webUi = readCredential(configDir, "web-ui");
    expect(tui).not.toBe(webUi);

    const anonymous = await request.get("/api/settings");
    expect(anonymous.status()).toBe(401);
    expect((await anonymous.json()).error).toContain("client credential");

    expect(await whoami(request, api, tui)).toEqual({ status: 200, name: "tui" });

    const viaBearer = await request.get("/api/whoami", {
      headers: { Authorization: `Bearer ${webUi}` },
    });
    expect(viaBearer.status()).toBe(200);
    expect((await viaBearer.json()).client.name).toBe("web-ui");

    const settings = await request.get("/api/settings", { headers: { "X-CCTrace-Token": tui } });
    expect(settings.status()).toBe(200);
    const body = await settings.json();
    expect(body.api_auth_enabled).toBe(true);
    expect(body.api_auth_source).toBe("file");
    expect(body.clients.map((c: { name: string }) => c.name)).toEqual(
      expect.arrayContaining(["web-ui", "tui"]),
    );
    // Credentials are never listed.
    expect(JSON.stringify(body)).not.toContain(tui);
    expect(JSON.stringify(body)).not.toContain(webUi);

    expect((await whoami(request, api, forge(tui))).status).toBe(401);
  });

  test("static shell only hands out the web-ui cookie to an allowlisted Host", async () => {
    const spoofed = await rawGet(port, "/", { Host: "attacker.example:1421" });
    expect(spoofed.status).toBe(200);
    expect(spoofed.headers["set-cookie"]).toBeUndefined();

    const loopback = await rawGet(port, "/", { Host: `localhost:${port}` });
    expect(loopback.status).toBe(200);
    const cookie = (loopback.headers["set-cookie"] ?? []).join(";");
    expect(cookie).toContain(`cctrace_token=${readCredential(configDir, "web-ui")}`);
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Strict");
  });
});

test.describe("browser UI", () => {
  test("authenticates as web-ui via the server-set cookie and shows the session", async ({
    page,
    context,
    baseURL,
  }) => {
    await page.goto("/");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();

    const cookie = (await context.cookies(baseURL!)).find((c) => c.name === "cctrace_token");
    expect(cookie?.value).toBe(readCredential(configDir, "web-ui"));
    expect(cookie?.httpOnly).toBe(true);
    expect(cookie?.sameSite).toBe("Strict");

    await page.getByText(FIXTURE_FIRST_MESSAGE).click();
    await expect(page.getByText(FIXTURE_REPLY)).toBeVisible();
  });

  test("re-acquires the credential when the browser reloads a cached shell", async ({
    page,
    context,
    baseURL,
  }) => {
    await page.goto("/");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();

    // The cookie is session-scoped, so it dies when the browser closes while
    // the cached HTML shell outlives it. Reproduce that pairing: drop the
    // cookie, keep the cache, reload. The shell is served `no-cache` so the
    // browser revalidates rather than reusing it blind, and the response —
    // `304` included — carries the credential again.
    await context.clearCookies();
    expect(await context.cookies(baseURL!)).toHaveLength(0);
    await page.reload();

    const cookie = (await context.cookies(baseURL!)).find((c) => c.name === "cctrace_token");
    expect(cookie?.value).toBe(readCredential(configDir, "web-ui"));
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
  });

  test("marks the shell no-cache so a cached copy can never starve a tab of its cookie", async () => {
    const shell = await rawGet(port, "/", { Host: `localhost:${port}` });
    expect(shell.status).toBe(200);
    expect(shell.headers["cache-control"]).toBe("no-cache");
  });

  test("keeps a long message list scrollable to the bottom at 150%", async ({ page }) => {
    writeScrollableSession(projectsDir, 80);
    await page.setViewportSize({ width: 1200, height: 800 });
    await page.goto("/");
    await page.getByText(SCROLL_FIXTURE_FIRST_MESSAGE).click();

    const list = page.locator(".message-list");
    const main = page.locator(".main-content");
    await expect(list).toBeVisible();
    await expect.poll(() => list.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
    await expect.poll(() => list.evaluate((el) => getComputedStyle(el).minHeight)).toBe("0px");

    const [listBox, mainBox] = await Promise.all([list.boundingBox(), main.boundingBox()]);
    expect(listBox).not.toBeNull();
    expect(mainBox).not.toBeNull();
    expect(listBox!.y + listBox!.height).toBeLessThanOrEqual(mainBox!.y + mainBox!.height + 1);

    const before = await list.evaluate((el) => el.scrollTop);
    await list.hover();
    await page.mouse.wheel(0, -500);
    await expect.poll(() => list.evaluate((el) => el.scrollTop)).not.toBe(before);

    await page.getByTitle("Settings").click();
    await page
      .getByRole("group", { name: "Font size" })
      .getByRole("button", { name: "150%" })
      .click();
    await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("1.5");
    await page.locator(".popout-modal__close").click();

    const lastMessage = page.locator(".message", { hasText: "scroll fixture 79:" });
    const expectLastMessageFullyVisible = async () => {
      await expect(lastMessage).toBeInViewport();
      const [lastBox, zoomedListBox] = await Promise.all([
        lastMessage.boundingBox(),
        list.boundingBox(),
      ]);
      expect(lastBox).not.toBeNull();
      expect(zoomedListBox).not.toBeNull();
      expect(lastBox!.y + lastBox!.height).toBeLessThanOrEqual(
        Math.min(zoomedListBox!.y + zoomedListBox!.height, page.viewportSize()!.height) + 1,
      );
    };

    await list.hover();
    await page.mouse.wheel(0, 100_000);
    await expectLastMessageFullyVisible();

    // The persisted scale must be in place before Virtuoso's first measurement
    // on the next launch/page load, not applied one frame after it mounts.
    await page.reload();
    await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("1.5");
    await page.getByText(SCROLL_FIXTURE_FIRST_MESSAGE).click();
    await list.hover();
    await page.mouse.wheel(0, 100_000);
    await expectLastMessageFullyVisible();
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
    // has no credential for the API calls that follow. (`route.fetch()` would
    // go through the browser's cookie jar and store the cookie anyway.)
    await context.clearCookies();
    await page.route("**/", async (route) => {
      const body = await (await fetch(route.request().url())).text();
      await route.fulfill({ status: 200, contentType: "text/html", body });
    });
    await page.goto("/");
    await expect(page.getByRole("alert")).toContainText("Not an accepted client");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toHaveCount(0);
  });

  test("Settings registers, reissues and revokes clients, each with its own credential", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
    await openSettings(page);
    const table = page.getByRole("table", { name: "Accepted clients" });
    await expect(table.getByText("web-ui", { exact: true })).toBeVisible();
    await expect(table.getByText("tui", { exact: true })).toBeVisible();

    // Register: the credential is shown once and identifies the new client.
    const name = `ci-script-${Date.now().toString(36)}`;
    const credential = await addClient(page, name);
    expect(await whoami(request, api, credential)).toEqual({ status: 200, name });
    expectSecretOnTestPath(configDir);
    const listed = await listClients(request, api, credential);
    expect(listed.find((c) => c.name === name)?.builtin).toBe(false);
    expect(JSON.stringify(listed)).not.toContain(credential);

    // Reissue a built-in: the file the TUI reads is rewritten, the old one dies.
    const oldTui = readCredential(configDir, "tui");
    const newTui = await reissueClient(page, "tui");
    expect(newTui).not.toBe(oldTui);
    expect(readCredential(configDir, "tui")).toBe(newTui);
    expect((await whoami(request, api, oldTui)).status).toBe(401);
    expect(await whoami(request, api, newTui)).toEqual({ status: 200, name: "tui" });

    // Revoke: only this client is cut off.
    await revokeClient(page, name);
    expect((await whoami(request, api, credential)).status).toBe(401);
    expect(await whoami(request, api, newTui)).toEqual({ status: 200, name: "tui" });
    await expect(table.getByRole("row", { name: new RegExp(name) })).toContainText("Revoked");
  });

  test("reissuing web-ui in the tab rotates its cookie and a reload still works", async ({
    page,
    context,
    request,
    baseURL,
  }) => {
    const oldWebUi = readCredential(configDir, "web-ui");
    await page.goto("/");
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();

    await openSettings(page);
    const newWebUi = await reissueClient(page, "web-ui");
    expect(newWebUi).not.toBe(oldWebUi);
    expect(readCredential(configDir, "web-ui")).toBe(newWebUi);
    expectSecretOnTestPath(configDir);

    // The reissue response re-set the cookie on this tab…
    const cookie = (await context.cookies(baseURL!)).find((c) => c.name === "cctrace_token");
    expect(cookie?.value).toBe(newWebUi);
    // …the old credential is dead for everyone…
    expect((await whoami(request, api, oldWebUi)).status).toBe(401);
    // …and a reload keeps working.
    await page.reload();
    await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
  });
});

test.describe("revoking web-ui", () => {
  test("locks browsers out (no cookie, banner) until another client reissues it", async ({
    browser,
    request,
  }) => {
    await ensureWebUiActive(request, api, configDir);
    const tui = readCredential(configDir, "tui");
    const page = await (await browser.newContext()).newPage();
    try {
      await page.goto("/");
      await expect(page.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
      await openSettings(page);
      // Two-click confirm; arming warns about exactly what this test proves.
      const revoke = page.getByRole("button", { name: "Revoke web-ui", exact: true });
      await revoke.click();
      await expect(page.getByText(/locks every browser tab/)).toBeVisible();
      await revoke.click();
      await expect(page.getByText('"web-ui" revoked')).toBeVisible();

      const shell = await rawGet(port, "/", { Host: `localhost:${port}` });
      expect(shell.status).toBe(200);
      expect(shell.headers["set-cookie"]).toBeUndefined();

      const fresh = await (await browser.newContext()).newPage();
      await fresh.goto("/");
      await expect(fresh.getByRole("alert")).toContainText("Not an accepted client");
      await fresh.context().close();
    } finally {
      // Another client (here: the TUI's credential, as the desktop app or a
      // script would) reissues web-ui, and the browser is back in.
      const restored = await reissueViaApi(request, api, tui, "web-ui");
      expect(readCredential(configDir, "web-ui")).toBe(restored);
      await page.context().close();
    }
    const back = await (await browser.newContext()).newPage();
    await back.goto("/");
    await expect(back.getByText(FIXTURE_FIRST_MESSAGE)).toBeVisible();
    await back.context().close();
  });
});
