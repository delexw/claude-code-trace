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

# The TUI is a Python/Textual app under tui-py/. Pre-build its virtualenv (tui-py/.venv)
# and install dependencies now via the same helper the launcher uses at runtime
# (bin/python-venv.mjs), so the first `cctrace --tui` starts instantly. Best-effort:
# Python 3 is a runtime prerequisite for the TUI only, so a missing interpreter must not
# fail a desktop/web install — warn and carry on; the launcher retries on first run.
echo "==> Setting up Python TUI dependencies (tui-py/.venv)..."
if ! node -e 'import("./bin/python-venv.mjs").then((m) => m.ensureTuiVenv(process.cwd()))'; then
  echo "    Skipped: no usable Python 3 found. Install Python 3 to enable 'cctrace --tui'." >&2
fi

echo "==> Linking cctrace CLI..."
npm link

echo ""
echo "Installed! Run:"
echo "  cctrace          # desktop app (default)"
echo "  cctrace --web    # web mode (opens browser)"
echo "  cctrace --tui    # terminal UI (sets up the Python venv on first run; needs Python 3)"
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo ""
  echo "You can also launch the installed app from /Applications/${APP_NAME}.app"
fi
