#!/usr/bin/env bash
set -euo pipefail

# Install claude-code-trace binary + TUI.
# Builds the frontend, installs the Rust binary to ~/.cargo/bin,
# and links the TUI as a global npm command.

cd "$(dirname "$0")/.."

echo "==> Installing npm dependencies..."
npm install

echo "==> Building frontend..."
npm run build

echo "==> Installing binary via cargo..."
cargo install --path src-tauri

echo "==> Building TUI..."
cd tui
npm install
npm run build
cd ..

echo "==> Linking cctrace CLI..."
npm link

echo ""
echo "Installed! Run:"
echo "  cctrace          # desktop app (default)"
echo "  cctrace --web    # web mode (opens browser)"
echo "  cctrace --tui    # terminal UI"

# --- Optional: install background web server ---
echo ""
read -rp "Would you like to run 'cctrace --web' as a background service on login? [y/N] " answer
if [[ "${answer,,}" == "y" ]]; then
  CCTRACE_BIN="$(command -v cctrace || echo "$HOME/.cargo/bin/cctrace")"

  case "$(uname -s)" in
    Darwin)
      PLIST="$HOME/Library/LaunchAgents/com.cctrace.web.plist"
      cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.cctrace.web</string>
  <key>ProgramArguments</key>
  <array>
    <string>${CCTRACE_BIN}</string>
    <string>--web</string>
    <string>--no-open</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>${HOME}/.claude/cctrace-web.log</string>
  <key>StandardErrorPath</key>
  <string>${HOME}/.claude/cctrace-web.log</string>
</dict>
</plist>
EOF
      launchctl unload "$PLIST" 2>/dev/null || true
      launchctl load "$PLIST"
      echo "Started! cctrace --web is now running and will start on login."
      echo "  Logs:    ~/.claude/cctrace-web.log"
      echo "  Stop:    launchctl unload $PLIST"
      echo "  Remove:  rm $PLIST"
      sleep 2 && open "http://localhost:1420" &
      ;;

    Linux)
      UNIT_DIR="$HOME/.config/systemd/user"
      mkdir -p "$UNIT_DIR"
      UNIT="$UNIT_DIR/cctrace-web.service"
      cat > "$UNIT" <<EOF
[Unit]
Description=cctrace web server
After=network.target

[Service]
ExecStart=${CCTRACE_BIN} --web --no-open
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
      systemctl --user daemon-reload
      systemctl --user enable --now cctrace-web.service
      echo "Started! cctrace --web is now running and will start on login."
      echo "  Logs:    journalctl --user -u cctrace-web -f"
      echo "  Stop:    systemctl --user stop cctrace-web"
      echo "  Remove:  systemctl --user disable cctrace-web && rm $UNIT"
      sleep 2 && xdg-open "http://localhost:1420" 2>/dev/null &
      ;;

    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      STARTUP_DIR="$(cmd.exe /c 'echo %APPDATA%' 2>/dev/null | tr -d '\r')/Microsoft/Windows/Start Menu/Programs/Startup"
      if [[ -d "$STARTUP_DIR" ]]; then
        VBS="$STARTUP_DIR/cctrace-web.vbs"
        CCTRACE_WIN="$(cygpath -w "$CCTRACE_BIN" 2>/dev/null || echo "$CCTRACE_BIN")"
        cat > "$VBS" <<EOF
Set WshShell = CreateObject("WScript.Shell")
WshShell.Run """${CCTRACE_WIN}"" --web --no-open", 0, False
EOF
        echo "Created startup script. cctrace --web will start on login."
        echo "  Remove: delete $VBS"
        # Start it now and open browser
        wscript.exe "$(cygpath -w "$VBS")" 2>/dev/null || true
        sleep 2 && cmd.exe /c start "http://localhost:1420" 2>/dev/null &
        echo "Started!"
      else
        echo "Could not find Windows Startup folder. You can manually add cctrace --web to Task Scheduler."
      fi
      ;;

    *)
      echo "Unsupported OS for background service. You can run 'cctrace --web &' manually."
      ;;
  esac
fi
