import { describe, it, expect, beforeEach, vi } from "vitest";

// Tests run in jsdom (no __TAURI_INTERNALS__), so the HTTP fallback is used.

const API_BASE = "http://127.0.0.1:11423";

function mockFetch(body: unknown, ok = true) {
  const fn = vi.fn().mockResolvedValue({
    ok,
    status: ok ? 200 : 400,
    statusText: ok ? "OK" : "Bad Request",
    text: () => Promise.resolve(JSON.stringify(body)),
    json: () => Promise.resolve(body),
  });
  vi.stubGlobal("fetch", fn);
  return fn;
}

describe("invoke (web/HTTP mode)", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("get_settings calls GET /api/settings", async () => {
    const data = { projects_dir: "/custom", default_dir: "/home/.claude/projects" };
    const fetchFn = mockFetch(data);
    const { invoke } = await import("./invoke");

    const res = await invoke<typeof data>("get_settings");
    expect(res).toEqual(data);
    expect(fetchFn).toHaveBeenCalledWith(
      `${API_BASE}/api/settings`,
      expect.objectContaining({
        headers: expect.objectContaining({ "Content-Type": "application/json" }),
      }),
    );
  });

  it("set_projects_dir calls POST /api/settings/dir", async () => {
    const data = { projects_dir: "/new", default_dir: "/home/.claude/projects" };
    const fetchFn = mockFetch(data);
    const { invoke } = await import("./invoke");

    await invoke("set_projects_dir", { path: "/new" });
    expect(fetchFn).toHaveBeenCalledWith(
      `${API_BASE}/api/settings/dir`,
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ path: "/new" }),
      }),
    );
  });

  it("get_project_dirs calls GET /api/project-dirs", async () => {
    const dirs = ["/a", "/b"];
    mockFetch(dirs);
    const { invoke } = await import("./invoke");

    const res = await invoke<string[]>("get_project_dirs");
    expect(res).toEqual(dirs);
  });

  it("list_wsl_distros calls GET /api/wsl/distros", async () => {
    const distros = ["Ubuntu", "Debian"];
    mockFetch(distros);
    const { invoke } = await import("./invoke");

    const res = await invoke<string[]>("list_wsl_distros");
    expect(res).toEqual(distros);
  });

  it("set_wsl_distros calls POST /api/wsl/distros with distros body", async () => {
    const fetchFn = mockFetch({ wsl_distros: ["Ubuntu"] });
    const { invoke } = await import("./invoke");

    await invoke("set_wsl_distros", { distros: ["Ubuntu"] });
    expect(fetchFn).toHaveBeenCalledWith(
      `${API_BASE}/api/wsl/distros`,
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ distros: ["Ubuntu"] }),
      }),
    );
  });

  it("discover_sessions calls POST /api/sessions with dirs body", async () => {
    const fetchFn = mockFetch([]);
    const { invoke } = await import("./invoke");

    await invoke("discover_sessions", { projectDirs: ["/a", "/b"] });
    const url = fetchFn.mock.calls[0][0] as string;
    expect(url).toBe(`${API_BASE}/api/sessions`);
    expect(fetchFn).toHaveBeenCalledWith(
      `${API_BASE}/api/sessions`,
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ dirs: ["/a", "/b"] }),
      }),
    );
  });

  it("load_session posts path + window (start/limit) to /api/session/load", async () => {
    const fetchFn = mockFetch({ messages: [], count: 0, start: 0, roles: [] });
    const { invoke } = await import("./invoke");

    await invoke("load_session", { path: "/a.jsonl", start: 100, limit: 50 });
    expect(fetchFn).toHaveBeenCalledWith(
      `${API_BASE}/api/session/load`,
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ path: "/a.jsonl", start: 100, limit: 50 }),
      }),
    );
  });

  it("load_message posts path + index to /api/session/message", async () => {
    const fetchFn = mockFetch({ role: "claude" });
    const { invoke } = await import("./invoke");

    await invoke("load_message", { path: "/a.jsonl", index: 42 });
    expect(fetchFn).toHaveBeenCalledWith(
      `${API_BASE}/api/session/message`,
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ path: "/a.jsonl", index: 42 }),
      }),
    );
  });

  it("watch/unwatch commands resolve without error", async () => {
    mockFetch({ ok: true });
    const { invoke } = await import("./invoke");

    await expect(invoke("watch_session", { path: "/a" })).resolves.toBeDefined();
    await expect(invoke("unwatch_session")).resolves.toBeDefined();
    await expect(invoke("watch_picker", { projectDirs: [] })).resolves.toBeDefined();
    await expect(invoke("unwatch_picker")).resolves.toBeDefined();
  });

  it("throws on HTTP error response", async () => {
    mockFetch({ error: "path does not exist" }, false);
    const { invoke } = await import("./invoke");

    await expect(invoke("set_projects_dir", { path: "/bad" })).rejects.toThrow(
      "path does not exist",
    );
  });

  it("unknown command throws", async () => {
    const { invoke } = await import("./invoke");
    await expect(invoke("nonexistent_cmd")).rejects.toThrow('Unknown command "nonexistent_cmd"');
  });
});
