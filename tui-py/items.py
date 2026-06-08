"""Item icon/name/summary helpers matching tui/src/lib/items.ts."""

from __future__ import annotations

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
            # Full prose renders inline in the item body, so a collapsed-row preview
            # would just duplicate the start of it.
            return ""
        case "HookEvent":
            if item.hook_name:
                cmd_part = f": {item.hook_command}" if item.hook_command else ""
                return f"{item.hook_name}{cmd_part}"
            return item.hook_command or ""
        case _:
            return ""
