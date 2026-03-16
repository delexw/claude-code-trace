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
npm link
cd ..

echo ""
echo "Installed! Run:"
echo "  claude-code-trace        # desktop app"
echo "  claude-code-trace --web  # web mode (opens browser)"
echo "  cctrace-tui              # terminal UI (requires backend running)"
