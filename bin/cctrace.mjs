#!/usr/bin/env node
import { execSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { createConnection } from "node:net";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { platform } from "node:os";
import { ensureTuiVenv } from "./python-venv.mjs";

const binDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(binDir, "..");

const args = process.argv.slice(2);
const mode = args.find((a) => ["--app", "--web", "--tui"].includes(a)) ?? "--app";
const noOpen = args.includes("--no-open");

function run(cmd, cmdArgs, opts = {}) {
  const child = spawn(cmd, cmdArgs, {
    stdio: "inherit",
    cwd: root,
    ...opts,
  });
  child.on("exit", (code) => process.exit(code ?? 0));
  return child;
}

function ask(question) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((res) => {
    rl.question(question, (answer) => {
      rl.close();
      res(answer.trim().toLowerCase());
    });
  });
}

/** Check if a port is already in use. */
function isPortInUse(port) {
  return new Promise((res) => {
    const sock = createConnection({ port, host: "127.0.0.1" });
    sock.once("connect", () => {
      sock.destroy();
      res(true);
    });
    sock.once("error", () => res(false));
  });
}

/** Poll until a port is accepting connections or the timeout is reached. */
async function waitForPort(port, timeoutMs = 30_000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    // eslint-disable-next-line no-await-in-loop -- sequential polling loop; Promise.all() is not applicable
    if (await isPortInUse(port)) return;
    // eslint-disable-next-line no-await-in-loop -- sequential polling loop; Promise.all() is not applicable
    await new Promise((r) => setTimeout(r, 300));
  }
}

/** Open a URL in the default browser. */
function openBrowser(url) {
  try {
    const os = platform();
    if (os === "darwin") execSync(`open "${url}"`);
    else if (os === "win32") execSync(`cmd.exe /c start "" "${url}"`);
    else execSync(`xdg-open "${url}" 2>/dev/null`);
  } catch {}
}

switch (mode) {
  case "--app":
    run("npx", ["tauri", "dev"]);
    break;

  case "--web": {
    // Check if the frontend (Vite, 1420) is already running.
    const frontendRunning = await isPortInUse(1420);
    // Check if the backend API (11423) is already running (e.g. desktop app).
    const backendRunning = await isPortInUse(11423);

    if (noOpen) {
      if (frontendRunning) {
        // Another instance owns port 1420 — exit cleanly so launchd
        // doesn't keep respawning us in a crash loop.
        console.error("Port 1420 already in use, exiting.");
        process.exit(0);
      }
      if (backendRunning) {
        // Backend already running — start Vite only (no Tauri).
        run("npx", ["vite"]);
      } else {
        run("npx", ["tauri", "dev", "--", "--", "--web", "--no-open"]);
      }
    } else {
      if (frontendRunning) {
        console.log("cctrace web server is already running on http://localhost:1420");
        openBrowser("http://localhost:1420");
        process.exit(0);
      }

      if (backendRunning) {
        // Backend already running (e.g. desktop app) — start Vite only.
        // The browser frontend will use the HTTP API at 11423 directly.
        console.log("Backend already running, starting Vite frontend only...");
        spawn("npx", ["vite"], { stdio: "inherit", cwd: root }).on("exit", (code) =>
          process.exit(code ?? 0),
        );
        await waitForPort(1420);
        openBrowser("http://localhost:1420");
      } else if (process.stdin.isTTY) {
        // Interactive — ask how to start.
        console.log("How would you like to start the web server?\n");
        console.log("  1) Start now (foreground, stops when you close the terminal)");
        console.log("  2) Install as background service (starts on login, always running)\n");

        const answer = await ask("Choose [1/2]: ");

        if (answer === "2") {
          const { installService } = await import("./install-service.mjs");
          installService();
          // Poll until the server is actually ready, then open browser.
          execSync("node wait-for-backend.mjs", {
            stdio: "inherit",
            cwd: resolve(root, "bin"),
          });
          openBrowser("http://localhost:1420");
        } else {
          run("npx", ["tauri", "dev", "--", "--", "--web"]);
        }
      } else {
        // Non-interactive — just start.
        run("npx", ["tauri", "dev", "--", "--", "--web"]);
      }
    }
    break;
  }

  case "--tui": {
    // If the backend (port 11423) is already running, connect to it instead
    // of starting a new headless instance.
    const backendRunning = await isPortInUse(11423);
    let backend = null;

    if (backendRunning) {
      console.log("Connecting to existing backend on http://127.0.0.1:11423");
    } else {
      // If port 1420 is already occupied by another Vite/web instance, pass a
      // different port via VITE_PORT so the headless Vite doesn't conflict.
      // The headless backend doesn't serve a browser UI, so any port is fine.
      const vitePort = (await isPortInUse(1420)) ? "0" : "";
      // `detached: true` makes this process the leader of its own process
      // group, so the whole tree it spawns (npx -> tauri CLI -> Vite + the
      // Rust backend) shares that group and can be killed together below.
      // Without it, backend.kill() only signals the immediate npx process —
      // Vite and the Rust backend survive as orphans still holding their
      // ports, which is why a later `cctrace --web` failed with EADDRINUSE.
      backend = spawn("npx", ["tauri", "dev", "--", "--", "--headless"], {
        stdio: "inherit",
        cwd: root,
        env: { ...process.env, ...(vitePort ? { VITE_PORT: vitePort } : {}) },
        detached: true,
      });
    }

    // Set up an isolated venv + install deps, wait for backend, then start TUI.
    const venvPython = ensureTuiVenv(root);
    execSync("node wait-for-backend.mjs", {
      stdio: "inherit",
      cwd: resolve(root, "bin"),
    });

    const tui = spawn(venvPython, ["main.py"], {
      stdio: "inherit",
      cwd: resolve(root, "tui-py"),
    });

    tui.on("exit", (code) => {
      killBackendTree(backend);
      process.exit(code ?? 0);
    });
    if (backend) backend.on("exit", () => tui.kill());
    break;
  }
}

/** Kill a detached backend's whole process group (npx -> tauri CLI -> Vite +
 * Rust backend), not just the immediate child — see the spawn() call above. */
function killBackendTree(backend) {
  if (!backend) return;
  try {
    process.kill(-backend.pid, "SIGTERM");
  } catch {
    // Group already gone (e.g. backend process exited on its own first).
  }
}
