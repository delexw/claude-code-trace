#!/usr/bin/env node
import { execSync, spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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

switch (mode) {
  case "--app":
    run("npx", ["tauri", "dev"]);
    break;

  case "--web": {
    const webArgs = ["tauri", "dev", "--", "--", "--web"];
    if (noOpen) webArgs.push("--no-open");
    run("npx", webArgs);
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
