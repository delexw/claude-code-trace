"""Color constants aligned with Textual's default dark theme palette."""

from __future__ import annotations

# Shared spinner frames used by all animated widgets
SPIN = ["◐", "◓", "◑", "◒"]

# ---------------------------------------------------------------------------
# Primary palette — mapped to Textual default dark theme CSS variables
# $text      = #DFE0E2   $text-muted  = #9EA1A5   $text-disabled = #696D74
# $accent    = #0065FF   $primary     = #004578   $secondary     = #0077CC
# $success   = #00AA44   $warning     = #FE8800   $error         = #FE0044
# $border    = #004578   $text-accent = #569AFF
# ---------------------------------------------------------------------------

TEXT_PRIMARY = "#DFE0E2"  # $text
TEXT_SECONDARY = "#9EA1A5"  # $text-muted
TEXT_DIM = "#9EA1A5"  # $text-muted
TEXT_MUTED = "#696D74"  # $text-disabled

ACCENT = "#569AFF"  # $text-accent (bright blue, readable on dark)
ACCENT_ALT = "#0077CC"  # $secondary
ACCENT_PURPLE = "#0077CC"  # $secondary
INFO = "#569AFF"  # $text-accent

BORDER = "#004578"  # $border / $primary

MODEL_OPUS = "#FE8800"  # $warning  (warm orange)
MODEL_SONNET = "#569AFF"  # $text-accent (blue)
MODEL_HAIKU = "#00AA44"  # $success (green)

TOKEN_HIGH = "#FE8800"  # $warning

ONGOING = "#00AA44"  # $success
ERROR = "#FE0044"  # $error

CONTEXT_OK = "#00AA44"  # $success
CONTEXT_WARN = "#FE8800"  # $warning
CONTEXT_CRIT = "#FE0044"  # $error

PILL_BYPASS = "#FE0044"  # $error
PILL_ACCEPT_EDITS = "#0077CC"  # $secondary
PILL_PLAN = "#00AA44"  # $success

GIT_BRANCH = "#FE8800"  # $warning (orange — stands out without clashing)

PICKER_SELECTED_BG = "#004578"  # $primary

ROLE_USER = "#569AFF"  # $text-accent (blue)
ROLE_CLAUDE = "#9EA1A5"  # $text-muted (neutral — Claude is the default)
ROLE_SYSTEM = "#696D74"  # $text-disabled

ITEM_THINKING = "#696D74"  # $text-disabled (dim — thinking is internal)
ITEM_OUTPUT = "#DFE0E2"  # $text (plain output)
ITEM_TOOL = "#569AFF"  # $text-accent (blue — tools are actions)
ITEM_TOOL_ERROR = "#FE0044"  # $error
ITEM_AGENT = "#0077CC"  # $secondary (purple-blue — agents)
ITEM_TEAMMATE = "#569AFF"  # $text-accent
ITEM_HOOK = "#FE8800"  # $warning (orange — hooks are side effects)

# ---------------------------------------------------------------------------
# Team colors — mapped to Textual semantic colors
# ---------------------------------------------------------------------------

TEAM_COLORS: dict[str, str] = {
    "blue": "#569AFF",  # $text-accent
    "green": "#00AA44",  # $success
    "red": "#FE0044",  # $error
    "yellow": "#FE8800",  # $warning
    "purple": "#0077CC",  # $secondary
    "cyan": "#569AFF",  # $text-accent
    "orange": "#FE8800",  # $warning
    "pink": "#9EA1A5",  # $text-muted
}


# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------


def get_model_color(model: str) -> str:
    if "opus" in model:
        return MODEL_OPUS
    if "sonnet" in model:
        return MODEL_SONNET
    if "haiku" in model:
        return MODEL_HAIKU
    return TEXT_SECONDARY


def get_team_color(name: str) -> str:
    return TEAM_COLORS.get(name.lower(), ACCENT)


def get_context_color(pct: int) -> str:
    if pct < 50:
        return CONTEXT_OK
    if pct < 80:
        return CONTEXT_WARN
    return CONTEXT_CRIT


def get_role_border_color(role: str, is_error: bool) -> str:
    if is_error:
        return ERROR
    if role == "user":
        return ROLE_USER
    if role == "claude":
        return ROLE_CLAUDE
    if role == "system":
        return ROLE_SYSTEM
    return BORDER


def get_item_color(item_type: str, has_error: bool) -> str:
    if item_type == "Thinking":
        return ITEM_THINKING
    if item_type == "Output":
        return ITEM_OUTPUT
    if item_type == "ToolCall":
        return ITEM_TOOL_ERROR if has_error else ITEM_TOOL
    if item_type == "Subagent":
        return ITEM_AGENT
    if item_type == "TeammateMessage":
        return ITEM_TEAMMATE
    if item_type == "HookEvent":
        return ITEM_HOOK
    return TEXT_DIM


def mode_color(mode: str) -> str:
    if mode == "bypassPermissions":
        return PILL_BYPASS
    if mode == "acceptEdits":
        return PILL_ACCEPT_EDITS
    if mode == "plan":
        return PILL_PLAN
    return TEXT_PRIMARY
