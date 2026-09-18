#!/usr/bin/env bash
set -euo pipefail

# Install the pre-built Claude Code Trace desktop app on macOS without cloning
# the repo and without the usual `xattr -cr` dance.
#
# The app is unsigned, so anything downloaded through a browser gets tagged with
# com.apple.quarantine and Gatekeeper refuses to open it. Quarantine is applied
# by the downloading app, not by the artifact — curl does not set it, so
# fetching the release tarball here and unpacking it into /Applications
# produces a bundle macOS launches normally.
#
# This installs the desktop app only. `cctrace --web` and `cctrace --tui` are
# launched from a source checkout — see script/install.sh for that.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/delexw/claude-code-trace/main/script/install-macos.sh | bash
#
# Environment overrides:
#   CCTRACE_VERSION      release tag to install (default: latest), e.g. v0.15.1
#   CCTRACE_INSTALL_DIR  destination directory (default: /Applications)
#   CCTRACE_BASE_URL     release download base (default: GitHub releases)

APP_NAME="Claude Code Trace"
REPO="delexw/claude-code-trace"
INSTALL_DIR="${CCTRACE_INSTALL_DIR:-/Applications}"
BASE_URL="${CCTRACE_BASE_URL:-https://github.com/${REPO}/releases}"

die() {
  echo "Error: $*" >&2
  exit 1
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  die "this installer is macOS-only. For Linux and Windows builds see https://github.com/${REPO}/releases"
fi

# On an Apple Silicon Mac running this script under Rosetta, `uname -m` reports
# x86_64. Ask the kernel whether we are translated before deciding there is no
# build for this machine.
arch="$(uname -m)"
if [[ "$arch" == "x86_64" && "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" == "1" ]]; then
  arch="arm64"
fi

case "$arch" in
  arm64) asset="Claude.Code.Trace_aarch64.app.tar.gz" ;;
  *)
    die "no pre-built app is published for ${arch} (Apple Silicon only).
Build from source instead:
  git clone https://github.com/${REPO}.git && cd claude-code-trace && ./script/install.sh"
    ;;
esac

if [[ -n "${CCTRACE_VERSION:-}" ]]; then
  url="${BASE_URL}/download/${CCTRACE_VERSION}/${asset}"
else
  url="${BASE_URL}/latest/download/${asset}"
fi

[[ -d "$INSTALL_DIR" ]] || die "install directory does not exist: $INSTALL_DIR"
[[ -w "$INSTALL_DIR" ]] || die "no write permission for $INSTALL_DIR. Re-run with sudo, or set CCTRACE_INSTALL_DIR to a writable location."

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

echo "==> Downloading ${APP_NAME} (${arch})..."
curl -fsSL --retry 3 -o "$workdir/app.tar.gz" "$url" ||
  die "download failed: $url"

echo "==> Unpacking..."
tar -xzf "$workdir/app.tar.gz" -C "$workdir" || die "could not unpack the downloaded archive"

staged="$workdir/${APP_NAME}.app"
[[ -d "$staged" ]] || die "archive did not contain ${APP_NAME}.app"
[[ -x "$staged/Contents/MacOS/claude-code-trace" ]] || die "unpacked bundle is missing its executable"

target="${INSTALL_DIR}/${APP_NAME}.app"
was_running=""
if pgrep -f "${APP_NAME}.app/Contents/MacOS/claude-code-trace" >/dev/null 2>&1; then
  was_running="yes"
fi

echo "==> Installing to ${target}..."
rm -rf "$target"
cp -R "$staged" "$INSTALL_DIR/"

echo ""
echo "Installed! Launch it from Spotlight or:"
echo "  open \"${target}\""
if [[ -n "$was_running" ]]; then
  echo ""
  echo "A previous instance was running — quit and reopen it to pick up this version."
fi
