import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("node:os", () => {
  const platform = vi.fn(() => "linux");
  const homedir = vi.fn(() => "/home/u");
  return { platform, homedir, default: { platform, homedir } };
});
vi.mock("node:fs", () => {
  const fns = { readFileSync: vi.fn(), writeFileSync: vi.fn(), mkdirSync: vi.fn() };
  return { ...fns, default: fns };
});

const { platform } = await import("node:os");
const { readFileSync, writeFileSync, mkdirSync } = await import("node:fs");
const { configDir, apiTokenPath, resolveApiToken } = await import("./api-token.mjs");

const enoent = () => {
  const e = new Error("ENOENT");
  e.code = "ENOENT";
  throw e;
};

beforeEach(() => {
  vi.clearAllMocks();
  platform.mockReturnValue("linux");
  readFileSync.mockImplementation(enoent);
});

describe("configDir", () => {
  it("uses XDG_CONFIG_HOME on linux when set", () => {
    expect(configDir({ platform: "linux", env: { XDG_CONFIG_HOME: "/xdg" }, home: "/h" })).toBe(
      "/xdg",
    );
  });

  it("falls back to ~/.config on linux", () => {
    expect(configDir({ platform: "linux", env: {}, home: "/h" })).toBe("/h/.config");
  });

  it("uses ~/Library/Application Support on darwin", () => {
    expect(configDir({ platform: "darwin", env: {}, home: "/Users/x" })).toBe(
      "/Users/x/Library/Application Support",
    );
  });

  it("uses APPDATA on win32, falling back to AppData/Roaming", () => {
    expect(configDir({ platform: "win32", env: { APPDATA: "C:/appdata" }, home: "C:/u" })).toBe(
      "C:/appdata",
    );
    // join() on this (posix) test host uses "/" separators; only the tail matters.
    expect(configDir({ platform: "win32", env: {}, home: "/u" })).toBe("/u/AppData/Roaming");
  });

  it("defaults to the running platform and home", () => {
    expect(configDir({ env: {} })).toBe("/home/u/.config");
  });
});

describe("apiTokenPath", () => {
  it("is the api-token file inside the app's config dir", () => {
    expect(apiTokenPath({ platform: "linux", env: {}, home: "/h" })).toBe(
      "/h/.config/claude-code-trace/api-token",
    );
  });
});

describe("resolveApiToken", () => {
  const opts = { platform: "linux", home: "/h" };
  const path = "/h/.config/claude-code-trace/api-token";

  it("returns null when CCTRACE_API_AUTH=off, without touching the file", () => {
    expect(resolveApiToken({ ...opts, env: { CCTRACE_API_AUTH: " OFF " } })).toBeNull();
    expect(readFileSync).not.toHaveBeenCalled();
    expect(writeFileSync).not.toHaveBeenCalled();
  });

  it("prefers CCTRACE_API_TOKEN over the file", () => {
    readFileSync.mockReturnValue("filetoken\n");
    expect(resolveApiToken({ ...opts, env: { CCTRACE_API_TOKEN: " envtoken " } })).toBe("envtoken");
    expect(readFileSync).not.toHaveBeenCalled();
  });

  it("reads and trims an existing token file", () => {
    readFileSync.mockReturnValue("  abc123 \n");
    expect(resolveApiToken({ ...opts, env: {} })).toBe("abc123");
    expect(readFileSync).toHaveBeenCalledWith(path, "utf8");
    expect(writeFileSync).not.toHaveBeenCalled();
  });

  it("creates a 64-hex token with O_EXCL and mode 0600 when the file is missing", () => {
    const token = resolveApiToken({ ...opts, env: {} });
    expect(token).toMatch(/^[0-9a-f]{64}$/);
    expect(mkdirSync).toHaveBeenCalledWith("/h/.config/claude-code-trace", { recursive: true });
    expect(writeFileSync).toHaveBeenCalledWith(path, `${token}\n`, { flag: "wx", mode: 0o600 });
  });

  it("re-reads the winner's token when the backend created the file first (EEXIST)", () => {
    readFileSync.mockImplementationOnce(enoent).mockImplementation(() => "winner\n");
    writeFileSync.mockImplementation(() => {
      const e = new Error("EEXIST");
      e.code = "EEXIST";
      throw e;
    });
    expect(resolveApiToken({ ...opts, env: {} })).toBe("winner");
  });

  it("rethrows unexpected write errors", () => {
    writeFileSync.mockImplementation(() => {
      const e = new Error("EACCES");
      e.code = "EACCES";
      throw e;
    });
    expect(() => resolveApiToken({ ...opts, env: {} })).toThrow("EACCES");
  });
});
