import { execSync } from "node:child_process";
import { resolve, delimiter } from "node:path";
import { platform } from "node:os";
import { existsSync, rmSync, readdirSync } from "node:fs";

/**
 * Extract the minor version from a python3.<minor> path so candidates can be
 * sorted newest-first. Bare `python3` / `python` have no minor and sort last.
 */
export function pythonMinorVersion(p) {
  const m = /python3\.(\d+)/.exec(p);
  return m ? Number(m[1]) : -1; // bare python3 / python sorts last
}

/**
 * Discover every Python 3 interpreter available on this machine by scanning the
 * directories on PATH (plus a few well-known install locations) for executables
 * named `python3` or `python3.<minor>`. Versions are discovered dynamically — no
 * hardcoded list — so future Python releases and whatever the user has installed
 * are picked up automatically. Returns absolute paths, newest minor version
 * first, with bare `python3` / `python` last.
 */
export function discoverPythonCandidates() {
  const isWin = platform() === "win32";
  const re = isWin ? /^python(3(\.\d+)?)?\.exe$/i : /^python3(\.\d+)?$/;
  const dirs = [
    ...(process.env.PATH ?? "").split(delimiter),
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
  ].filter(Boolean);

  const found = new Set();
  for (const dir of dirs) {
    let entries;
    try {
      entries = readdirSync(dir);
    } catch {
      continue; // dir missing or unreadable
    }
    for (const name of entries) {
      if (re.test(name)) found.add(resolve(dir, name));
    }
  }

  return [...found].toSorted((a, b) => pythonMinorVersion(b) - pythonMinorVersion(a));
}

/**
 * Ensure a dedicated virtualenv exists for the TUI with its dependencies
 * installed, and return the path to the venv's python executable.
 *
 * Using a dedicated venv (instead of bare `pip` + `python3`) guarantees that
 * dependency install and app launch use the SAME interpreter, isolated from
 * whatever python the user's shell happens to resolve — asdf shims, an active
 * unrelated virtualenv, system python, etc. The venv is created once and
 * reused on subsequent launches.
 *
 * Each discovered interpreter is validated by actually creating the venv and
 * confirming pip is present: some interpreters can `import ensurepip` yet still
 * fail to seed pip (broken bundled wheels, externally-managed installs, a
 * pip-less parent venv). On failure we discard the half-built venv and fall
 * through to the next candidate.
 *
 * @param {string} root - the project root containing the `tui-py` directory
 * @returns {string} absolute path to the venv's python executable
 */
export function ensureTuiVenv(root) {
  const tuiDir = resolve(root, "tui-py");
  const venvDir = resolve(tuiDir, ".venv");
  const isWin = platform() === "win32";
  const venvPython = resolve(venvDir, isWin ? "Scripts/python.exe" : "bin/python");

  if (!existsSync(venvPython)) {
    console.log("Creating Python virtualenv for the TUI (tui-py/.venv)...");
    const candidates = discoverPythonCandidates();
    let created = false;
    for (const base of candidates) {
      try {
        execSync(`"${base}" -m venv "${venvDir}"`, { stdio: "ignore" });
        execSync(`"${venvPython}" -m pip --version`, { stdio: "ignore" });
        created = true;
        break;
      } catch {
        rmSync(venvDir, { recursive: true, force: true });
      }
    }
    if (!created) {
      console.error(
        "Could not find a Python 3 interpreter able to create a virtualenv with pip.\n" +
          "Install Python 3 (e.g. `brew install python` on macOS) and try again.",
      );
      process.exit(1);
    }
  }

  execSync(
    `"${venvPython}" -m pip install -r requirements.txt --quiet --disable-pip-version-check`,
    { stdio: "inherit", cwd: tuiDir },
  );

  return venvPython;
}
