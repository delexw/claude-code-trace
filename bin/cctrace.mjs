#!/usr/bin/env node
import { execSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { createConnection } from "node:net";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { platform } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

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

/** Wait until the server responds to HTTP, with a timeout. */
function waitForServer(url, timeoutMs = 120000) {
  const start = Date.now();
  return new Promise((res) => {
    async function check() {
      if (Date.now() - start > timeoutMs) {
        // Give up but still open — server might be partially ready.
        res();
        return;
      }
      try {
        const resp = await fetch(url, { signal: AbortSignal.timeout(10000) });
        if (resp.ok) {
          res();
          return;
        }
      } catch {}
      setTimeout(check, 1000);
    }
    check();
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
    // Check if the server is already running on port 1420.
    const alreadyRunning = await isPortInUse(1420);

    if (noOpen) {
      if (alreadyRunning) {
        // Another instance owns the port — exit cleanly so launchd
        // doesn't keep respawning us in a crash loop.
        console.error("Port 1420 already in use, exiting.");
        process.exit(0);
      }
      run("npx", ["tauri", "dev", "--", "--", "--web", "--no-open"]);
    } else {
      if (alreadyRunning) {
        console.log("cctrace web server is already running on http://localhost:1420");
        openBrowser("http://localhost:1420");
        process.exit(0);
      }

      if (process.stdin.isTTY) {
        // Interactive — ask how to start.
        console.log("How would you like to start the web server?\n");
        console.log("  1) Start now (foreground, stops when you close the terminal)");
        console.log("  2) Install as background service (starts on login, always running)\n");

        const answer = await ask("Choose [1/2]: ");

        if (answer === "2") {
          const { installService } = await import("./install-service.mjs");
          installService();
          // Poll until the server is actually ready, then open browser.
          console.log("Waiting for server to start...");
          await waitForServer("http://localhost:1420");
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
    // Start backend headless, then launch TUI once API is ready
    const backend = spawn("npx", ["tauri", "dev", "--", "--", "--headless"], {
      stdio: "inherit",
      cwd: root,
    });

    // Build TUI if needed, wait for backend, then start TUI
    execSync("npm run build", { stdio: "inherit", cwd: resolve(root, "tui") });
    execSync("node wait-for-backend.mjs", {
      stdio: "inherit",
      cwd: resolve(root, "tui"),
    });

    const tui = spawn("node", ["dist/tui/src/cli.js"], {
      stdio: "inherit",
      cwd: resolve(root, "tui"),
    });

    tui.on("exit", (code) => {
      backend.kill();
      process.exit(code ?? 0);
    });
    backend.on("exit", () => tui.kill());
    break;
  }
}
