"""DetailView widget — expandable item list for a selected message.

Uses a ListView for item navigation (same widget as MessageList/SessionPicker)
so j/k, --highlight styling, and focus management are consistent across all pages.
Message content is rendered as a collapsible header above the list.
"""

from __future__ import annotations

import contextlib

from rich.cells import cell_len
from rich.text import Text
from textual.app import ComposeResult
from textual.widget import Widget
from textual.widgets import Collapsible, ListItem, Markdown, Static

import theme
from data_types import DisplayItem, DisplayMessage
from diff_utils import compute_edit_diff
from format_utils import format_duration, format_tokens, role_icon
from items import get_item_icon, get_item_name, get_item_summary
from theme import get_item_color, get_role_border_color, get_team_color
from widgets.highlight_list import HighlightListView

# Icons
ICON_BAR_SINGLE = "│"
ICON_BAR_DOUBLE = "┃"
ICON_THINKING = "◆"
ICON_TOOL = "⚙"


# ---------------------------------------------------------------------------
# Rendering helpers (pure functions, no widget state)
# ---------------------------------------------------------------------------


def _has_inline_prose(msg: DisplayMessage) -> bool:
    """True when the turn has Output items that render the assistant prose inline.

    Output items render their prose in chronological position alongside the tool
    calls, so the flattened ``msg.content`` blob (and its title preview) would just
    duplicate the same text out of order.
    """
    return any(it.item_type == "Output" for it in msg.items)


def _item_collapsed(item: DisplayItem, idx: int, expanded_items: set[int]) -> bool:
    """Whether an item's Collapsible starts collapsed.

    Output is the assistant's prose: it renders inline always so a turn reads as
    commentary interleaved with tool calls in chronological order. Every other type
    stays collapsed until the user expands it.
    """
    if item.item_type == "Output":
        return False
    return idx not in expanded_items


def _stats_text_parts(msg: DisplayMessage) -> list[tuple[str, str]]:
    import format_utils as fu

    parts: list[tuple[str, str]] = []
    if msg.thinking_count > 0:
        parts.append((f"{ICON_THINKING}{msg.thinking_count}", theme.ITEM_THINKING))
    if msg.tool_call_count > 0:
        parts.append((f"{ICON_TOOL}{msg.tool_call_count}", theme.ITEM_TOOL))
    if msg.tokens_raw > 0:
        tok_clr = theme.TOKEN_HIGH if msg.tokens_raw > 150000 else theme.TEXT_DIM
        parts.append((fu.format_tokens(msg.tokens_raw), tok_clr))
    if msg.duration_ms > 0:
        parts.append((fu.format_duration(msg.duration_ms), theme.TEXT_DIM))
    return parts


def _render_msg_title(msg: DisplayMessage, depth: int, ongoing: bool, anim_frame: int = 0) -> str:
    """Build a Rich markup string for the message Collapsible title."""
    role_name = "Claude" if msg.role == "claude" else "User" if msg.role == "user" else "System"
    icon = role_icon(msg.role)
    border_clr = get_role_border_color(msg.role, msg.is_error)
    role_clr = (
        theme.ROLE_CLAUDE
        if msg.role == "claude"
        else theme.ROLE_USER
        if msg.role == "user"
        else theme.ROLE_SYSTEM
    )

    parts: list[str] = []
    if depth > 0:
        parts.append(f"[{theme.TEXT_DIM}]{'▸ ' * depth}[/]")
    parts.append(f"[bold {border_clr}]{icon} [/][{role_clr}]{role_name}[/]")
    if msg.subagent_label:
        parts.append(f"  [{theme.ITEM_AGENT}][{msg.subagent_label}][/]")
    if ongoing:
        parts.append(f"  [{theme.ONGOING}]{theme.SPIN[anim_frame]}[/]")

    if msg.content and not _has_inline_prose(msg):
        first_line = next((ln.strip() for ln in msg.content.split("\n") if ln.strip()), "")
        if first_line:
            preview = first_line[:80] + ("…" if len(first_line) > 80 else "")
            parts.append(f"  [{theme.TEXT_DIM}]{preview}[/]")

    stats: list[str] = []
    for text, clr in _stats_text_parts(msg):
        stats.append(f"[{clr}]{text}[/]")
    if stats:
        sep = f" [{theme.TEXT_MUTED}]·[/] "
        parts.append("  " + sep.join(stats))

    return "".join(parts)


def _render_item_title(item: DisplayItem, max_name_len: int = 12) -> str:
    """Render a markup string for a Collapsible item title."""
    has_agent = len(item.subagent_messages) > 0
    team_clr = get_team_color(item.team_color) if item.team_color else None
    item_clr = (
        team_clr
        if (has_agent and team_clr)
        else get_item_color(item.item_type, bool(item.tool_error))
    )
    bar = ICON_BAR_DOUBLE if has_agent else ICON_BAR_SINGLE
    icon_str = get_item_icon(item)
    icon_str = icon_str + " " * (2 - cell_len(icon_str))
    name_str = get_item_name(item).ljust(max_name_len)
    summary = get_item_summary(item)

    parts = [f"[{item_clr}]{bar} {icon_str}[/][bold {item_clr}]{name_str}[/]"]
    if summary:
        parts.append(f" [{theme.TEXT_DIM}]— {summary}[/]")

    right_parts: list[str] = []
    if item.token_count > 0:
        right_parts.append(f"[{theme.TEXT_DIM}]{format_tokens(item.token_count)}[/]")
    if item.duration_ms > 0:
        right_parts.append(f"[{theme.TEXT_DIM}]{format_duration(item.duration_ms)}[/]")
    if has_agent:
        clr = team_clr or theme.ITEM_AGENT
        right_parts.append(f"[{clr}][{len(item.subagent_messages)} msg][/]")

    return "".join(parts) + ("  " + " · ".join(right_parts) if right_parts else "")


def _md_json(s: str) -> str:
    """Try to render as a JSON fenced block; fall back to plain text if not valid JSON."""
    import json as _json

    try:
        pretty = _json.dumps(_json.loads(s), indent=2)
        return f"```json\n{pretty}\n```"
    except Exception:
        return s


def _md_code(s: str, lang: str = "") -> str:
    return f"```{lang}\n{s}\n```"


# Per-line foreground colour for each diff line kind.
_DIFF_LINE_STYLE = {
    "context": theme.TEXT_MUTED,
    "removed": "#f85149",
    "added": "#3fb950",
}
# Stronger style (bold + background tint) for the exact changed words within a
# removed/added line. Context lines never carry word-level changes.
_DIFF_WORD_STYLE = {
    "removed": "bold #f85149 on #67060c",
    "added": "bold #3fb950 on #033a16",
}
_DIFF_MARKER = {"context": " ", "removed": "-", "added": "+"}


def _render_edit_diff(tool_input: str) -> Text | None:
    """Render an Edit tool input as a coloured Rich diff, or None.

    Unchanged lines are kept as context; removed/added lines are coloured
    red/green and the exact changed words carry a stronger background tint so
    the intra-line edit is visible without Markdown's (uncoloured) diff fence.
    """
    import json as _json

    try:
        parsed = _json.loads(tool_input)
    except Exception:
        return None
    if not isinstance(parsed, dict):
        return None
    file_path = parsed.get("file_path", "")
    old_string = parsed.get("old_string")
    new_string = parsed.get("new_string")
    if not isinstance(old_string, str) or not isinstance(new_string, str):
        return None

    text = Text()
    if file_path:
        text.append(file_path, style=f"bold {theme.ACCENT}")
        if parsed.get("replace_all"):
            text.append("  (replace all)", style=theme.TEXT_DIM)
        text.append("\n")

    diff_lines = compute_edit_diff(old_string.split("\n"), new_string.split("\n"))
    last = len(diff_lines) - 1
    for i, dl in enumerate(diff_lines):
        line_style = _DIFF_LINE_STYLE[dl.kind]
        word_style = _DIFF_WORD_STYLE.get(dl.kind)
        text.append(_DIFF_MARKER[dl.kind], style=line_style)
        for seg in dl.segments:
            style = word_style if (seg.changed and word_style) else line_style
            text.append(seg.text, style=style)
        if i < last:
            text.append("\n")
    return text


def _tool_result_md(item: DisplayItem) -> str:
    """Markdown for a tool call's Result/Error section, or "" when absent."""
    if not (item.tool_result or item.tool_result_json):
        return ""
    label = "**Error**" if item.tool_error else "**Result**"
    if item.tool_result_json:
        return f"{label}\n\n```json\n{item.tool_result_json}\n```"
    return f"{label}\n\n{_md_json(item.tool_result)}"


def _render_item_body(item: DisplayItem) -> str:
    """Render the full expanded body of an item as Markdown."""
    match item.item_type:
        case "Thinking":
            return item.text or "*Thinking content is not recorded in session logs.*"
        case "Output":
            return _md_json(item.text) if item.text else ""
        case "ToolCall":
            parts: list[str] = []
            if item.tool_input:
                parts.append("**Input**")
                parts.append(_md_json(item.tool_input))
            parts.append(_tool_result_md(item))
            return "\n\n".join(p for p in parts if p)
        case "Subagent":
            parts = []
            if item.agent_id:
                parts.append(f"**id:** `{item.agent_id}`")
            if item.subagent_desc:
                parts.append(f"**description:** {item.subagent_desc}")
            if item.subagent_prompt:
                parts.append(f"**prompt**\n\n{_md_code(item.subagent_prompt)}")
            if item.text:
                parts.append(f"**Result**\n\n{item.text}")
            return "\n\n".join(parts)
        case "TeammateMessage":
            return item.text
        case "HookEvent":
            parts = [f"**hook:** `{item.hook_event}` · `{item.hook_name}`"]
            if item.hook_command:
                parts.append(f"**cmd**\n\n{_md_json(item.hook_command)}")
            if item.hook_metadata:
                parts.append(f"**metadata**\n\n{_md_json(item.hook_metadata)}")
            return "\n\n".join(parts)
        case _:
            return item.text


def _item_body_widgets(item: DisplayItem) -> list[Widget]:
    """Widgets to mount inside an item's Collapsible.

    Edit tool calls render a coloured Rich diff (red/green lines + highlighted
    changed words) as a Static, since Markdown can't colour a diff. Everything
    else is a single Markdown widget.
    """
    if item.item_type == "ToolCall" and item.tool_name == "Edit" and item.tool_input:
        diff = _render_edit_diff(item.tool_input)
        if diff is not None:
            widgets: list[Widget] = [Markdown("**Input**"), Static(diff, classes="diff-block")]
            result_md = _tool_result_md(item)
            if result_md:
                widgets.append(Markdown(result_md))
            return widgets
    return [Markdown(_render_item_body(item))]


# ---------------------------------------------------------------------------
# Inner ListView — carries BASE_BINDINGS so Footer shows them when focused
# ---------------------------------------------------------------------------


class _ItemListView(HighlightListView):
    """ListView of item Collapsibles. Highlight CSS comes from HighlightListView.

    Collapsible / CollapsibleTitle backgrounds are forced transparent so
    the shared ListItem.-highlight background actually shows through.
    """

    DEFAULT_CSS = """
    _ItemListView {
        height: 1fr;
    }
    _ItemListView ListItem {
        padding: 0;
        height: auto;
        background: transparent;
    }
    _ItemListView Collapsible,
    _ItemListView CollapsibleTitle {
        background: transparent;
    }
    _ItemListView Markdown {
        border: round $border;
        padding: 0 1;
        margin: 0 1;
    }
    _ItemListView Static.diff-block {
        border: round $border;
        padding: 0 1;
        margin: 0 1;
        width: 1fr;
    }
    """


# ---------------------------------------------------------------------------
# DetailView
# ---------------------------------------------------------------------------


class DetailView(Widget):
    """Renders a selected message: collapsible header + ListView of items.

    Layout:
      - Collapsible  (#msg-content)  — message role/content, always first
      - _ItemListView (#items-list)  — one ListItem(Collapsible) per DisplayItem
    """

    DEFAULT_CSS = """
    DetailView {
        height: 100%;
        layout: vertical;
        border: round $border;
        padding: 0 1;
    }
    .section-heading {
        height: 1;
        padding: 0 1;
        margin: 0 0 1 0;
        color: $text-muted;
        text-style: bold;
    }
    #msg-content {
        height: auto;
        padding: 0;
        margin: 0 0 1 0;
        border-bottom: solid $border;
    }
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._message: DisplayMessage | None = None
        self._items: list[DisplayItem] = []
        self._expanded_items: set[int] = set()
        self._ongoing: bool = False
        self._depth: int = 0
        self._max_name_len: int = 12
        self._anim_frame: int = 0

    def compose(self) -> ComposeResult:
        yield Static("── RESPONSE ──", id="msg-heading", classes="section-heading")
        yield Collapsible(
            Static(""),
            title="",
            collapsed=False,
            id="msg-content",
        )
        yield Static("", id="items-heading", classes="section-heading")
        yield _ItemListView(id="items-list")

    def on_mount(self) -> None:
        self.set_interval(0.5, self._spin)

    def _spin(self) -> None:
        with contextlib.suppress(Exception):
            if not self._ongoing or self._message is None:
                return
            self._anim_frame = (self._anim_frame + 1) % len(theme.SPIN)
            coll = self.query_one("#msg-content", Collapsible)
            coll.title = _render_msg_title(self._message, self._depth, True, self._anim_frame)

    # ----------------------------------------------------------------
    # Public API
    # ----------------------------------------------------------------

    def populate(
        self,
        message: DisplayMessage | None,
        expanded_items: set[int],
        ongoing: bool,
        depth: int,
    ) -> None:
        """Sync content for a (potentially new) message.

        When the items list is structurally unchanged (e.g. the caller just
        flipped an expand bit), we only update the existing Collapsibles in
        place — leaving the ListView selection where the user put it.
        Otherwise we do a full rebuild.
        """
        prev_items = self._items
        prev_message = self._message

        self._message = message
        self._expanded_items = expanded_items
        self._ongoing = ongoing
        self._depth = depth
        new_items = message.items or [] if message is not None else []
        self._items = new_items

        if new_items:
            self._max_name_len = min(24, max(12, max(len(get_item_name(it)) for it in new_items)))
        else:
            self._max_name_len = 12

        items_unchanged = (
            prev_message is not None and message is not None and prev_items == new_items
        )
        if items_unchanged:
            self.call_after_refresh(self._sync_expanded_only)
        else:
            # Eagerly clear the previous message's items + content so the user
            # never sees stale data while the view switches and the async
            # _rebuild is still pending. The LoadingIndicator covers the
            # whole pane until _rebuild finishes.
            with contextlib.suppress(Exception):
                self.query_one("#items-list", _ItemListView).clear()
            with contextlib.suppress(Exception):
                msg_coll = self.query_one("#msg-content", Collapsible)
                msg_coll.title = ""
                msg_coll.remove_children()
            self.loading = True
            self.call_after_refresh(self._rebuild)

    async def _sync_expanded_only(self) -> None:
        """Update Collapsible states + msg title without touching the ListView.

        Used when the underlying items have not changed structurally — keeps
        the user's cursor position and focus intact across an Enter-toggle.
        """
        with contextlib.suppress(Exception):
            if self._message is not None:
                msg_coll = self.query_one("#msg-content", Collapsible)
                msg_coll.title = _render_msg_title(
                    self._message, self._depth, self._ongoing, self._anim_frame
                )

        for idx in range(len(self._items)):
            with contextlib.suppress(Exception):
                coll = self.query_one(f"#item-{idx}", Collapsible)
                coll.collapsed = _item_collapsed(self._items[idx], idx, self._expanded_items)

    async def _rebuild(self) -> None:
        try:
            # Update message content collapsible
            msg_coll = self.query_one("#msg-content", Collapsible)
            if self._message:
                msg_coll.title = _render_msg_title(
                    self._message, self._depth, self._ongoing, self._anim_frame
                )
                content = self._message.content or ""
                await msg_coll.remove_children()
                if _has_inline_prose(self._message):
                    # Output items render the prose inline below; don't duplicate it here.
                    await msg_coll.mount(Static(f"[{theme.TEXT_DIM}]Prose shown inline below[/]"))
                    msg_coll.collapsed = True
                elif content:
                    await msg_coll.mount(Markdown(content))
                    msg_coll.collapsed = False
                else:
                    await msg_coll.mount(Static(f"[{theme.TEXT_DIM}]No content[/]"))
                    msg_coll.collapsed = False
            else:
                msg_coll.title = f"[{theme.TEXT_DIM}]No message selected[/]"
                msg_coll.collapsed = True

            # Top section heading — hidden when no message is loaded.
            msg_heading = self.query_one("#msg-heading", Static)
            msg_heading.display = self._message is not None

            # STEP section heading between message body and items list.
            heading = self.query_one("#items-heading", Static)
            n_items = len(self._items)
            if n_items > 0:
                heading.update(f"── STEP ({n_items}) ──")
                heading.display = True
            else:
                heading.display = False

            # Rebuild items ListView
            lv = self.query_one("#items-list", _ItemListView)
            await lv.clear()

            for idx, item in enumerate(self._items):
                title = _render_item_title(item, self._max_name_len)
                collapsed = _item_collapsed(item, idx, self._expanded_items)
                await lv.append(
                    ListItem(
                        Collapsible(
                            *_item_body_widgets(item),
                            title=title,
                            collapsed=collapsed,
                            id=f"item-{idx}",
                        )
                    )
                )

            if self._items:
                lv.index = 0
                lv.focus()
        except Exception:
            pass
        finally:
            with contextlib.suppress(Exception):
                self.loading = False

    def current_item_index(self) -> int | None:
        with contextlib.suppress(Exception):
            lv = self.query_one("#items-list", _ItemListView)
            idx = lv.index
            if idx is not None and 0 <= idx < len(self._items):
                return idx
        return None

    def toggle_item(self, idx: int) -> None:
        with contextlib.suppress(Exception):
            coll = self.query_one(f"#item-{idx}", Collapsible)
            coll.collapsed = not coll.collapsed

    def expand_all(self) -> None:
        with contextlib.suppress(Exception):
            lv = self.query_one("#items-list", _ItemListView)
            for child in lv.children:
                for coll in child.query(Collapsible):
                    coll.collapsed = False

    def collapse_all(self) -> None:
        with contextlib.suppress(Exception):
            lv = self.query_one("#items-list", _ItemListView)
            for child in lv.children:
                for coll in child.query(Collapsible):
                    coll.collapsed = True

    def focus_item(self, idx: int) -> None:
        with contextlib.suppress(Exception):
            lv = self.query_one("#items-list", _ItemListView)
            if 0 <= idx < len(self._items):
                lv.index = idx
                lv.focus()
