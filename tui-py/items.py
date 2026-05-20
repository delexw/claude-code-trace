"""Item icon/name/summary helpers matching tui/src/lib/items.ts."""

from __future__ import annotations

import json

from data_types import DisplayItem

# ---------------------------------------------------------------------------
# Icons (matching icons.ts)
# ---------------------------------------------------------------------------

ICON_THINKING = "◆"  # U+25C6
ICON_OUTPUT = "▪"  # U+25AA
ICON_TOOL = "⚙"  # U+2699
ICON_SUBAGENT = "✦"  # U+2726
ICON_TEAMMATE = "◈"  # U+25C8
ICON_HOOK = "⚡"  # U+26A1
ICON_DOT = "·"  # U+00B7


def _json_shape_summary(text: str) -> str | None:
    """Returns a compact summary of a JSON string: '{key1, key2, …}' or '[N items]'."""
    try:
        parsed = json.loads(text)
        if isinstance(parsed, list):
            return f"[{len(parsed)} items]"
        if isinstance(parsed, dict) and parsed is not None:
            keys = list(parsed.keys())
            shown = ", ".join(keys[:4])
            suffix = ", …" if len(keys) > 4 else ""
            return "{" + shown + suffix + "}"
    except Exception:
        pass
    return None


def get_item_icon(item: DisplayItem) -> str:
    """Return a Unicode icon for a DisplayItem type."""
    match item.item_type:
        case "Thinking":
            return ICON_THINKING
        case "Output":
            return ICON_OUTPUT
        case "ToolCall":
            return ICON_TOOL
        case "Subagent":
            return ICON_SUBAGENT
        case "TeammateMessage":
            return ICON_TEAMMATE
        case "HookEvent":
            return ICON_HOOK
        case _:
            return ICON_DOT


def get_item_name(item: DisplayItem) -> str:
    """Return a display name for a DisplayItem."""
    match item.item_type:
        case "Thinking":
            return "Thinking"
        case "Output":
            return "Output"
        case "ToolCall":
            return item.tool_name or "Tool"
        case "Subagent":
            return item.subagent_type or "Subagent"
        case "TeammateMessage":
            return item.team_member_name or "Teammate"
        case "HookEvent":
            return item.hook_event or "Hook"
        case _:
            return item.item_type


def get_item_summary(item: DisplayItem) -> str:
    """Return a short summary for a DisplayItem."""
    match item.item_type:
        case "ToolCall":
            return item.tool_summary or ""
        case "Subagent":
            return item.subagent_desc or ""
        case "TeammateMessage":
            return item.text or ""
        case "Thinking":
            return item.text or "Content not recorded"
        case "Output":
            if not item.text:
                return ""
            return _json_shape_summary(item.text) or item.text
        case "HookEvent":
            if item.hook_name:
                cmd_part = f": {item.hook_command}" if item.hook_command else ""
                return f"{item.hook_name}{cmd_part}"
            return item.hook_command or ""
        case _:
            return ""
