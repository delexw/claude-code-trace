"""Shared HTTP API client token — TUI side.

The Rust backend (``src-tauri/src/auth.rs``) requires every ``/api/*`` request
to carry a secret token so that only accepted clients can reach it. This module
mirrors the backend's resolution so the TUI, running as the same OS user,
finds the same token:

1. ``CCTRACE_API_AUTH=off``  → no token (verification disabled)
2. ``CCTRACE_API_TOKEN=<t>`` → that token
3. ``<config dir>/claude-code-trace/api-token`` → read it (never created here —
   the backend owns the file).

``auth_headers()`` re-resolves on every call, so a token rotated from the
Settings UI is picked up on the TUI's next request without a restart.
"""

from __future__ import annotations

import os
import sys
from collections.abc import Mapping
from pathlib import Path

TOKEN_HEADER = "X-CCTrace-Token"
ENV_TOKEN = "CCTRACE_API_TOKEN"
ENV_AUTH = "CCTRACE_API_AUTH"
ENV_CONFIG_DIR = "CCTRACE_CONFIG_DIR"


def config_dir(
    platform: str | None = None,
    env: Mapping[str, str] | None = None,
    home: Path | None = None,
) -> Path:
    """Mirror of Rust's ``dirs::config_dir()`` for the three supported platforms."""
    platform = sys.platform if platform is None else platform
    env = os.environ if env is None else env
    home = Path.home() if home is None else home
    if platform.startswith("win"):
        appdata = env.get("APPDATA")
        return Path(appdata) if appdata else home / "AppData" / "Roaming"
    if platform == "darwin":
        return home / "Library" / "Application Support"
    xdg = env.get("XDG_CONFIG_HOME")
    return Path(xdg) if xdg else home / ".config"


def app_config_root(
    platform: str | None = None,
    env: Mapping[str, str] | None = None,
    home: Path | None = None,
) -> Path:
    """The app's config root: ``$CCTRACE_CONFIG_DIR`` when set (mirrors the backend's
    ``settings::config_root``), else ``<config dir>/claude-code-trace``."""
    env = os.environ if env is None else env
    override = env.get(ENV_CONFIG_DIR, "").strip()
    if override:
        return Path(override)
    return config_dir(platform, env, home) / "claude-code-trace"


def token_path(
    platform: str | None = None,
    env: Mapping[str, str] | None = None,
    home: Path | None = None,
) -> Path:
    """``<config root>/api-token`` — sibling of ``settings.json``."""
    return app_config_root(platform, env, home) / "api-token"


def resolve_api_token(env: Mapping[str, str] | None = None, path: Path | None = None) -> str | None:
    """The token to present, or ``None`` when disabled or not (yet) available."""
    env = os.environ if env is None else env
    if env.get(ENV_AUTH, "").strip().lower() == "off":
        return None
    from_env = env.get(ENV_TOKEN, "").strip()
    if from_env:
        return from_env
    path = token_path(env=env) if path is None else path
    try:
        text = path.read_text(encoding="utf-8").strip()
    except OSError:
        return None
    return text or None


def auth_headers(token: str | None = None) -> dict[str, str]:
    """Headers to send with every backend call (empty when no token applies)."""
    tok = resolve_api_token() if token is None else token
    return {TOKEN_HEADER: tok} if tok else {}
