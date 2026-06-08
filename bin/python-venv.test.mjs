import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("node:os", () => {
  const platform = vi.fn(() => "darwin");
  return { platform, default: { platform } };
});
vi.mock("node:fs", () => {
  const fns = { existsSync: vi.fn(), rmSync: vi.fn(), readdirSync: vi.fn(() => []) };
  return { ...fns, default: fns };
});
vi.mock("node:child_process", () => {
  const execSync = vi.fn();
  return { execSync, default: { execSync } };
});

const { platform } = await import("node:os");
const { existsSync, rmSync, readdirSync } = await import("node:fs");
const { execSync } = await import("node:child_process");
const { pythonMinorVersion, discoverPythonCandidates, ensureTuiVenv } =
  await import("./python-venv.mjs");

beforeEach(() => {
  vi.clearAllMocks();
  platform.mockReturnValue("darwin");
});

describe("pythonMinorVersion", () => {
  it("extracts the minor version from a python3.<minor> path", () => {
    expect(pythonMinorVersion("/usr/bin/python3.12")).toBe(12);
    expect(pythonMinorVersion("/opt/homebrew/bin/python3.9")).toBe(9);
  });

  it("returns -1 for bare python3 / python so they sort last", () => {
    expect(pythonMinorVersion("/usr/bin/python3")).toBe(-1);
    expect(pythonMinorVersion("/usr/bin/python")).toBe(-1);
  });
});

describe("discoverPythonCandidates", () => {
  const origPath = process.env.PATH;
  afterEach(() => {
    process.env.PATH = origPath;
  });

  it("scans PATH dirs, sorts newest-first, and puts bare python3 last", () => {
    process.env.PATH = "/fakebin";
    readdirSync.mockImplementation((dir) => {
      if (dir === "/fakebin") return ["python3", "python3.11", "python3.12", "node", "ruby"];
      throw new Error("ENOENT"); // well-known dirs absent on this test machine
    });

    const candidates = discoverPythonCandidates();

    expect(candidates).toEqual(["/fakebin/python3.12", "/fakebin/python3.11", "/fakebin/python3"]);
  });

  it("dedupes the same interpreter found via multiple dirs", () => {
    process.env.PATH = "/usr/bin";
    // /usr/bin is also in the hardcoded well-known list, so it is scanned twice.
    readdirSync.mockImplementation((dir) =>
      dir === "/usr/bin"
        ? ["python3.12"]
        : (() => {
            throw new Error("ENOENT");
          })(),
    );

    expect(discoverPythonCandidates()).toEqual(["/usr/bin/python3.12"]);
  });

  it("skips unreadable directories without throwing", () => {
    process.env.PATH = "/missing";
    readdirSync.mockImplementation(() => {
      throw new Error("ENOENT");
    });

    expect(discoverPythonCandidates()).toEqual([]);
  });

  it("matches python.exe / python3.exe on Windows", () => {
    // path.resolve/delimiter are host-specific (POSIX on the test machine), so
    // assert on the matched basenames rather than fully-resolved Windows paths.
    platform.mockReturnValue("win32");
    process.env.PATH = "winbin";
    readdirSync.mockImplementation((dir) =>
      dir === "winbin"
        ? ["python.exe", "python3.12.exe", "notepad.exe"]
        : (() => {
            throw new Error("ENOENT");
          })(),
    );

    const basenames = discoverPythonCandidates().map((p) => p.split("/").at(-1));
    expect(basenames).toContain("python3.12.exe");
    expect(basenames).toContain("python.exe");
    expect(basenames).not.toContain("notepad.exe");
  });
});

describe("ensureTuiVenv", () => {
  const root = "/proj";
  const venvPython = "/proj/tui-py/.venv/bin/python";

  it("reuses an existing venv and only runs pip install", () => {
    existsSync.mockReturnValue(true);

    const result = ensureTuiVenv(root);

    expect(result).toBe(venvPython);
    // No venv creation attempted.
    expect(execSync).toHaveBeenCalledTimes(1);
    expect(execSync).toHaveBeenCalledWith(
      `"${venvPython}" -m pip install -r requirements.txt --quiet --disable-pip-version-check`,
      expect.objectContaining({ cwd: "/proj/tui-py" }),
    );
    expect(rmSync).not.toHaveBeenCalled();
  });

  it("creates the venv with the first interpreter that yields working pip", () => {
    existsSync.mockReturnValue(false);
    process.env.PATH = "/fakebin";
    readdirSync.mockImplementation((dir) =>
      dir === "/fakebin"
        ? ["python3.12"]
        : (() => {
            throw new Error("ENOENT");
          })(),
    );
    execSync.mockReturnValue(""); // venv create + pip check + pip install all succeed

    const result = ensureTuiVenv(root);

    expect(result).toBe(venvPython);
    expect(execSync).toHaveBeenCalledWith(
      `"/fakebin/python3.12" -m venv "/proj/tui-py/.venv"`,
      expect.anything(),
    );
    expect(rmSync).not.toHaveBeenCalled();
  });

  it("discards a half-built venv and falls through to the next candidate", () => {
    existsSync.mockReturnValue(false);
    process.env.PATH = "/fakebin";
    readdirSync.mockImplementation((dir) =>
      dir === "/fakebin"
        ? ["python3.12", "python3.11"]
        : (() => {
            throw new Error("ENOENT");
          })(),
    );
    // The pip check uses the (identical) venv-python path for every candidate,
    // so distinguish by call order: the first candidate's pip check fails, the
    // second succeeds.
    let pipChecks = 0;
    execSync.mockImplementation((cmd) => {
      if (cmd.includes("-m pip --version")) {
        pipChecks += 1;
        if (pipChecks === 1) throw new Error("no pip");
      }
      return "";
    });

    const result = ensureTuiVenv(root);

    expect(result).toBe(venvPython);
    expect(rmSync).toHaveBeenCalledWith("/proj/tui-py/.venv", expect.anything());
    expect(execSync).toHaveBeenCalledWith(
      `"/fakebin/python3.11" -m venv "/proj/tui-py/.venv"`,
      expect.anything(),
    );
  });

  it("exits when no interpreter can produce a venv with pip", () => {
    existsSync.mockReturnValue(false);
    process.env.PATH = "/fakebin";
    readdirSync.mockImplementation((dir) =>
      dir === "/fakebin"
        ? ["python3.12"]
        : (() => {
            throw new Error("ENOENT");
          })(),
    );
    execSync.mockImplementation(() => {
      throw new Error("venv failed");
    });
    const exit = vi.spyOn(process, "exit").mockImplementation(() => {
      throw new Error("process.exit");
    });
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.spyOn(console, "log").mockImplementation(() => {});

    expect(() => ensureTuiVenv(root)).toThrow("process.exit");
    expect(exit).toHaveBeenCalledWith(1);
    expect(rmSync).toHaveBeenCalled();

    exit.mockRestore();
  });
});
