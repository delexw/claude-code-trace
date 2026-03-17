#!/usr/bin/env node
/**
 * Install cctrace --web as a background service on login.
 * Supports macOS (launchd), Linux (systemd), and Windows (Startup folder).
 *
 * Captures the current shell PATH so the service can find node, npx, cargo, etc.
 */
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";
import { execSync } from "node:child_process";

const LABEL = "com.cctrace.web";

function findBinary() {
  try {
    return execSync("command -v cctrace", { encoding: "utf8" }).trim();
  } catch {
    return join(homedir(), ".cargo", "bin", "cctrace");
  }
}

function isInstalled() {
  switch (platform()) {
    case "darwin":
      return existsSync(join(homedir(), "Library", "LaunchAgents", `${LABEL}.plist`));
    case "linux":
      return existsSync(join(homedir(), ".config", "systemd", "user", "cctrace-web.service"));
    case "win32": {
      const startup = join(
        process.env.APPDATA || join(homedir(), "AppData", "Roaming"),
        "Microsoft",
        "Windows",
        "Start Menu",
        "Programs",
        "Startup",
        "cctrace-web.vbs",
      );
      return existsSync(startup);
    }
    default:
      return false;
  }
}

function installDarwin(bin) {
  const dir = join(homedir(), "Library", "LaunchAgents");
  mkdirSync(dir, { recursive: true });
  const plist = join(dir, `${LABEL}.plist`);
  const logPath = join(homedir(), ".claude", "cctrace-web.log");
  const currentPath = process.env.PATH || "/usr/local/bin:/usr/bin:/bin";
  writeFileSync(
    plist,
    `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>${bin}</string>
    <string>--web</string>
    <string>--no-open</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>${currentPath}</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>${logPath}</string>
  <key>StandardErrorPath</key>
  <string>${logPath}</string>
</dict>
</plist>
`,
  );
  try {
    execSync(`launchctl unload "${plist}" 2>/dev/null`, { stdio: "ignore" });
  } catch {}
  execSync(`launchctl load "${plist}"`);
  console.error("Installed! cctrace --web will start on login.");
  console.error(`  Logs:   ${logPath}`);
  console.error(`  Stop:   launchctl unload "${plist}"`);
  console.error(`  Remove: rm "${plist}"`);
}

function installLinux(bin) {
  const dir = join(homedir(), ".config", "systemd", "user");
  mkdirSync(dir, { recursive: true });
  const unit = join(dir, "cctrace-web.service");
  const currentPath = process.env.PATH || "/usr/local/bin:/usr/bin:/bin";
  writeFileSync(
    unit,
    `[Unit]
Description=cctrace web server
After=network.target

[Service]
Environment=PATH=${currentPath}
ExecStart=${bin} --web --no-open
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
`,
  );
  execSync("systemctl --user daemon-reload");
  execSync("systemctl --user enable --now cctrace-web.service");
  console.error("Installed! cctrace --web will start on login.");
  console.error("  Logs:   journalctl --user -u cctrace-web -f");
  console.error("  Stop:   systemctl --user stop cctrace-web");
  console.error(`  Remove: systemctl --user disable cctrace-web && rm "${unit}"`);
}

function installWindows(bin) {
  const startupDir = join(
    process.env.APPDATA || join(homedir(), "AppData", "Roaming"),
    "Microsoft",
    "Windows",
    "Start Menu",
    "Programs",
    "Startup",
  );
  if (!existsSync(startupDir)) {
    console.error("Could not find Windows Startup folder.");
    console.error("You can manually add cctrace --web to Task Scheduler.");
    return;
  }
  const vbs = join(startupDir, "cctrace-web.vbs");
  const winBin = bin.replace(/\//g, "\\");
  writeFileSync(
    vbs,
    `Set WshShell = CreateObject("WScript.Shell")\nWshShell.Run """${winBin}"" --web --no-open", 0, False\n`,
  );
  console.error("Installed! cctrace --web will start on login.");
  console.error(`  Remove: delete "${vbs}"`);
}

export { isInstalled };

export function installService() {
  const bin = findBinary();

  switch (platform()) {
    case "darwin":
      installDarwin(bin);
      break;
    case "linux":
      installLinux(bin);
      break;
    case "win32":
      installWindows(bin);
      break;
    default:
      console.error(`Unsupported OS (${platform()}). Run 'cctrace --web &' manually.`);
  }
}
