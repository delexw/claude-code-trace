"""Formatting utilities matching the TypeScript shared/format.ts and tui/src/lib/format.ts."""

from __future__ import annotations

import json
import re
from datetime import UTC, datetime

# ---------------------------------------------------------------------------
# Token / cost / duration
# ---------------------------------------------------------------------------


def format_tokens(n: int) -> str:
    """Format a token count: 1234 -> '1.2k', 1234567 -> '1.2M'."""
    if n >= 1_000_000:
        return f"{n / 1_000_000:.1f}M"
    if n >= 1_000:
        return f"{n / 1_000:.1f}k"
    return str(n)


def format_cost(usd: float) -> str:
    """Format USD cost: 1.5 -> '$1.50'."""
    return f"${usd:.2f}"


def format_duration(ms: int) -> str:
    """Format duration: 1500 -> '1.5s', 90000 -> '1m 30s'."""
    if ms < 1000:
        return f"{ms}ms"
    s = ms / 1000
    if s < 60:
        return f"{s:.1f}s"
    m = int(s // 60)
    rem = int(s % 60)
    if rem > 0:
        return f"{m}m {rem}s"
    return f"{m}m"


# ---------------------------------------------------------------------------
# Model name
# ---------------------------------------------------------------------------


def short_model(m: str) -> str:
    """Turn 'claude-opus-4-6' into 'opus4.6'."""
    s = m.replace("claude-", "", 1)
    dash_idx = s.find("-")
    if dash_idx == -1:
        return s
    family = s[:dash_idx]
    rest = s[dash_idx + 1 :]
    v_parts = rest.split("-")
    if len(v_parts) >= 2:
        version = v_parts[0] + "." + v_parts[1]
    else:
        version = v_parts[0]
    return family + version


# ---------------------------------------------------------------------------
# Time
# ---------------------------------------------------------------------------


def time_ago(iso: str) -> str:
    """Return relative time: '3m ago', '2h ago', '5d ago'."""
    if not iso:
        return ""
    try:
        ts = datetime.fromisoformat(iso.replace("Z", "+00:00"))
        now = datetime.now(UTC)
        diff = now - ts
        mins = int(diff.total_seconds() / 60)
        if mins < 1:
            return "now"
        if mins < 60:
            return f"{mins}m ago"
        hrs = mins // 60
        if hrs < 24:
            return f"{hrs}h ago"
        days = hrs // 24
        return f"{days}d ago"
    except Exception:
        return ""


# ---------------------------------------------------------------------------
# String helpers
# ---------------------------------------------------------------------------


def truncate(s: str, n: int) -> str:
    """Truncate string to n chars, collapsing newlines, with ellipsis."""
    line = s.replace("\n", " ").strip()
    if len(line) > n:
        return line[: n - 1] + "…"
    return line


def first_line(s: str) -> str:
    """Return the first non-empty line of text."""
    idx = s.find("\n")
    return s if idx == -1 else s[:idx]


def format_json(s: str) -> str:
    """Pretty-print a JSON string; return original on failure."""
    try:
        return json.dumps(json.loads(s), indent=2)
    except Exception:
        return s


# ---------------------------------------------------------------------------
# Role helpers
# ---------------------------------------------------------------------------


def role_icon(role: str) -> str:
    if role == "claude":
        return "✦"  # ✦
    if role == "user":
        return "●"  # ●
    if role == "system":
        return "$"
    return " "


def role_color(role: str) -> str:
    from theme import ACCENT, TEXT_MUTED, TEXT_SECONDARY

    if role == "claude":
        return TEXT_SECONDARY
    if role == "user":
        return ACCENT
    if role == "system":
        return TEXT_MUTED
    return "#e2e8f0"


# ---------------------------------------------------------------------------
# Project key helpers (ported from shared/format.ts)
# ---------------------------------------------------------------------------


def project_key(path: str) -> str:
    """Extract the encoded project directory key from a session path."""
    m = re.search(r"[/\\]\.claude[/\\]projects[/\\]([^/\\]+)", path)
    return m.group(1) if m else "unknown"


def project_display_name(key: str) -> str:
    """Decode a project key to a display name (last path segment).

    Matches TypeScript: key.replace(/^-/, '/').replaceAll('-', '/')
    """
    # Strip leading dash, then replace all remaining dashes with slashes
    path = re.sub(r"^-", "/", key).replace("-", "/")
    parts = [p for p in path.split("/") if p]
    return parts[-1] if parts else key


def short_path(cwd: str) -> str:
    """Return the last path segment of cwd."""
    if not cwd:
        return ""
    parts = [p for p in cwd.split("/") if p]
    return parts[-1] if parts else cwd


# ---------------------------------------------------------------------------
# Context percent
# ---------------------------------------------------------------------------

CONTEXT_WINDOW_SIZE = 1_000_000


def context_percent(messages: list) -> int:
    """Return context usage % from the last claude message, or -1 if not available."""
    for msg in reversed(messages):
        if msg.role == "claude" and msg.context_tokens > 0:
            return min(int(msg.context_tokens * 100 / CONTEXT_WINDOW_SIZE), 100)
    return -1


# ---------------------------------------------------------------------------
# Permission mode display
# ---------------------------------------------------------------------------


def short_mode(mode: str) -> str:
    if mode == "acceptEdits":
        return "auto-edit"
    if mode == "bypassPermissions":
        return "yolo"
    if mode == "plan":
        return "plan"
    return mode
