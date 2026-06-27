#!/usr/bin/env bash
set -euo pipefail

# Install claude-code-trace + TUI.
#
# - macOS: builds a proper `.app` bundle via `tauri build` and installs it to
#   /Applications. A bare `cargo install` binary on macOS has no `.app` wrapper
#   or Info.plist (and thus no GUI activation policy), so its webview launches
#   as a blank white window.
# - Linux/other: installs the Rust binary to ~/.cargo/bin via `cargo install`.
#
# In all cases the `cctrace` CLI launcher (desktop/web/tui) is linked globally.

cd "$(dirname "$0")/.."

APP_NAME="Claude Code Trace"

echo "==> Installing npm dependencies..."
npm install

echo "==> Building frontend..."
npm run build

if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "==> Building macOS .app bundle via Tauri..."
  npx tauri build --bundles app

  APP_BUNDLE="src-tauri/target/release/bundle/macos/${APP_NAME}.app"
  if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "Error: expected app bundle not found at $APP_BUNDLE" >&2
    exit 1
  fi

  echo "==> Installing ${APP_NAME}.app to /Applications..."
  rm -rf "/Applications/${APP_NAME}.app"
  cp -R "$APP_BUNDLE" "/Applications/"

  # A previous `cargo install` may have left a bare binary on PATH that opens a
  # blank window on macOS. Remove it so the .app bundle is the only launch path.
  STALE_BIN="${CARGO_HOME:-$HOME/.cargo}/bin/claude-code-trace"
  if [[ -e "$STALE_BIN" ]]; then
    echo "==> Removing stale cargo binary at $STALE_BIN..."
    rm -f "$STALE_BIN"
  fi
else
  echo "==> Installing binary via cargo..."
  cargo install --path src-tauri
fi

if [[ -d tui ]]; then
  echo "==> Building TUI..."
  cd tui
  npm install
  npm run build
  cd ..
else
  echo "==> Skipping TUI build (tui directory not found)."
fi

echo "==> Linking cctrace CLI..."
npm link

echo ""
echo "Installed! Run:"
echo "  cctrace          # desktop app (default)"
echo "  cctrace --web    # web mode (opens browser)"
echo "  cctrace --tui    # terminal UI"
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo ""
  echo "You can also launch the installed app from /Applications/${APP_NAME}.app"
fi
