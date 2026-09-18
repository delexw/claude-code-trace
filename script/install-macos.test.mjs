import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

const script = resolve(dirname(fileURLToPath(import.meta.url)), "install-macos.sh");

let sandbox;
let fakeBin;
let installDir;
let fixture;
let urlLog;

/** Write an executable stub that shadows a real command on PATH. */
function stub(name, body) {
  const path = join(fakeBin, name);
  writeFileSync(path, `#!/usr/bin/env bash\n${body}\n`);
  chmodSync(path, 0o755);
}

/**
 * A tarball shaped like the published release asset. `shape` selects a full
 * bundle, one missing its executable, or an archive with no bundle at all.
 */
function buildFixture(shape = "full") {
  const staging = join(sandbox, `staging-${shape}`);
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });

  if (shape === "no-bundle") {
    writeFileSync(join(staging, "README.txt"), "wrong asset\n");
  } else {
    const app = join(staging, "Claude Code Trace.app", "Contents", "MacOS");
    mkdirSync(app, { recursive: true });
    if (shape === "full") {
      const exe = join(app, "claude-code-trace");
      writeFileSync(exe, "#!/usr/bin/env bash\nexit 0\n");
      chmodSync(exe, 0o755);
    }
  }

  const tarball = join(sandbox, `${shape}.tar.gz`);
  execFileSync("tar", ["-czf", tarball, "-C", staging, "."]);
  return tarball;
}

/** Point the curl stub at a different tarball. */
function serveFixture(tarball) {
  stub(
    "curl",
    `for a in "$@"; do [ -n "\${take:-}" ] && out="$a" && take=""; [ "$a" = "-o" ] && take=1; done
cp ${JSON.stringify(tarball)} "$out"`,
  );
}

function runInstaller(env = {}) {
  return execFileSync("bash", [script], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}:${process.env.PATH}`,
      CCTRACE_INSTALL_DIR: installDir,
      ...env,
    },
  });
}

beforeEach(() => {
  sandbox = mkdtempSync(join(tmpdir(), "cctrace-install-"));
  fakeBin = join(sandbox, "bin");
  installDir = join(sandbox, "Applications");
  urlLog = join(sandbox, "url.txt");
  mkdirSync(fakeBin, { recursive: true });
  mkdirSync(installDir, { recursive: true });
  fixture = buildFixture();

  stub("uname", 'case "$1" in -s) echo Darwin ;; -m) echo arm64 ;; esac');
  stub("sysctl", "echo 0");
  stub("pgrep", "exit 1");
  // Record the requested URL, then serve the fixture as the download.
  stub(
    "curl",
    `for a in "$@"; do [ -n "\${take:-}" ] && out="$a" && take=""; [ "$a" = "-o" ] && take=1; done
echo "\${@: -1}" > ${JSON.stringify(urlLog)}
cp ${JSON.stringify(fixture)} "$out"`,
  );
});

afterEach(() => {
  rmSync(sandbox, { recursive: true, force: true });
});

describe("install-macos.sh", () => {
  it("installs the app bundle into the install directory", () => {
    const out = runInstaller();
    expect(out).toContain("Installed!");
    const exe = join(installDir, "Claude Code Trace.app", "Contents", "MacOS", "claude-code-trace");
    expect(() => readFileSync(exe)).not.toThrow();
  });

  it("downloads the latest release by default", () => {
    runInstaller();
    expect(readFileSync(urlLog, "utf8").trim()).toBe(
      "https://github.com/delexw/claude-code-trace/releases/latest/download/Claude.Code.Trace_aarch64.app.tar.gz",
    );
  });

  it("downloads a pinned release when CCTRACE_VERSION is set", () => {
    runInstaller({ CCTRACE_VERSION: "v0.15.1" });
    expect(readFileSync(urlLog, "utf8").trim()).toBe(
      "https://github.com/delexw/claude-code-trace/releases/download/v0.15.1/Claude.Code.Trace_aarch64.app.tar.gz",
    );
  });

  it("replaces an existing install rather than nesting inside it", () => {
    runInstaller();
    runInstaller();
    const stale = join(installDir, "Claude Code Trace.app", "Claude Code Trace.app");
    expect(() => readFileSync(join(stale, "Contents", "MacOS", "claude-code-trace"))).toThrow();
  });

  it("tells the user to restart when an instance is already running", () => {
    stub("pgrep", "exit 0");
    expect(runInstaller()).toContain("quit and reopen");
  });

  it("refuses to run on a non-macOS platform", () => {
    stub("uname", 'case "$1" in -s) echo Linux ;; -m) echo x86_64 ;; esac');
    expect(() => runInstaller()).toThrow(/macOS-only/);
  });

  it("points Intel Macs at a source build", () => {
    stub("uname", 'case "$1" in -s) echo Darwin ;; -m) echo x86_64 ;; esac');
    expect(() => runInstaller()).toThrow(/no pre-built app is published for x86_64/);
  });

  it("treats a Rosetta-translated shell as Apple Silicon", () => {
    stub("uname", 'case "$1" in -s) echo Darwin ;; -m) echo x86_64 ;; esac');
    stub("sysctl", "echo 1");
    runInstaller();
    expect(readFileSync(urlLog, "utf8")).toContain("aarch64");
  });

  it("fails when the install directory is missing", () => {
    expect(() => runInstaller({ CCTRACE_INSTALL_DIR: join(sandbox, "nope") })).toThrow(
      /install directory does not exist/,
    );
  });

  it("fails when the archive has no app bundle", () => {
    serveFixture(buildFixture("no-bundle"));
    expect(() => runInstaller()).toThrow(/did not contain/);
  });

  it("fails when the bundle is missing its executable", () => {
    serveFixture(buildFixture("no-exe"));
    expect(() => runInstaller()).toThrow(/missing its executable/);
  });

  it("fails with a clear message when the download fails", () => {
    stub("curl", "exit 22");
    expect(() => runInstaller()).toThrow(/download failed/);
  });
});
