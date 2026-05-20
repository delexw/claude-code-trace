"""MessageList widget — full-width, responsive session message cards.

Design:
- Each card is a 3-column Rich Table (accent rail | content | right-aligned stats).
- No fixed box width — cards fill the terminal width automatically.
- Ongoing last message spins a progress indicator every 0.5 s via set_interval.
- Selected card uses Textual's --highlight CSS background.
"""

from __future__ import annotations

import contextlib

from rich.cells import cell_len
from rich.table import Table
from rich.text import Text
from textual.binding import Binding
from textual.widgets import ListItem, Static

import theme
from data_types import DisplayMessage
from format_utils import (
    first_line,
    format_duration,
    format_tokens,
    role_icon,
    short_model,
    truncate,
)
from items import get_item_icon, get_item_name
from theme import get_item_color, get_model_color, get_role_border_color, get_team_color
from widgets.highlight_list import HighlightListView

# Icons
ICON_THINKING = "◆"
ICON_TOOL = "⚙"
ICON_OUTPUT = "▪"
ICON_SUBAGENT = "✦"
ICON_SPAWN = "→"
ICON_HRULE = "─"


def _stats_right(msg: DisplayMessage) -> Text:
    """Build compact right-side stats as a Rich Text object."""
    parts: list[tuple[str, str]] = []

    if msg.thinking_count > 0:
        parts.append((f"{ICON_THINKING}{msg.thinking_count}", theme.ITEM_THINKING))
    if msg.tool_call_count > 0:
        parts.append((f"{ICON_TOOL}{msg.tool_call_count}", theme.ITEM_TOOL))
    if msg.output_count > 0:
        parts.append((f"{ICON_OUTPUT}{msg.output_count}", theme.TEXT_DIM))

    agent_count = sum(
        1 for it in msg.items if it.item_type == "Subagent" or len(it.subagent_messages) > 0
    )
    if agent_count > 0:
        parts.append((f"{ICON_SUBAGENT}{agent_count}", theme.ITEM_AGENT))
    if msg.teammate_spawns > 0:
        parts.append((f"{ICON_SPAWN}{msg.teammate_spawns}", theme.TEXT_DIM))

    if msg.tokens_raw > 0:
        tok_clr = theme.TOKEN_HIGH if msg.tokens_raw > 150000 else theme.TEXT_DIM
        parts.append((format_tokens(msg.tokens_raw), tok_clr))
    if msg.duration_ms > 0:
        parts.append((format_duration(msg.duration_ms), theme.TEXT_DIM))

    result = Text()
    for i, (text, style) in enumerate(parts):
        if i > 0:
            result.append(" · ", style=theme.TEXT_MUTED)
        result.append(text, style=style)
    return result


def _render_message(
    msg: DisplayMessage,
    idx: int,
    total: int,
    is_expanded: bool,
    is_last: bool,
    ongoing: bool,
    anim_frame: int = 0,
) -> object:
    """Return a Rich renderable for one message card.

    Layout uses a 3-column Table so the content fills the terminal width and
    stats are always right-aligned, regardless of terminal size.
    """
    # ── Special non-conversational roles ──────────────────────────────────────
    if msg.role == "compact":
        t = Text()
        t.append(f"{ICON_HRULE * 4} {msg.content} {ICON_HRULE * 4}", style=theme.TEXT_DIM)
        return t

    if msg.role == "system":
        t = Text()
        t.append(
            f"{ICON_HRULE * 2} $ System · {truncate(msg.content, 80)} {ICON_HRULE * 2}",
            style=theme.TEXT_MUTED,
        )
        return t

    if msg.role == "recap":
        t = Text()
        t.append(f"{ICON_HRULE * 4} Session Recap {ICON_HRULE * 4}\n", style=theme.TEXT_DIM)
        t.append(truncate(msg.content, 120), style=theme.TEXT_DIM)
        return t

    # ── Normal Claude / User message ──────────────────────────────────────────
    border_clr = get_role_border_color(msg.role, msg.is_error)
    header_clr = theme.ROLE_CLAUDE if msg.role == "claude" else theme.ROLE_USER
    model_str = short_model(msg.model) if msg.model else ""
    model_clr = get_model_color(msg.model) if msg.model else theme.TEXT_DIM

    # 3-column table: [rail 2ch] [content fills] [stats right-aligned]
    tbl = Table(
        show_header=False,
        box=None,
        padding=(0, 0),
        expand=True,
        show_edge=False,
    )
    tbl.add_column(width=2)  # accent rail
    tbl.add_column(ratio=1)
    tbl.add_column(justify="right", no_wrap=True)

    # ── Header row: role · model · spin · position on left; stats on right ──
    icon = role_icon(msg.role)
    role_name = "Claude" if msg.role == "claude" else "User"

    left = Text()
    left.append(f"{icon} ", style=f"bold {border_clr}")
    left.append(role_name, style=header_clr)
    if model_str:
        left.append(f"  {model_str}", style=model_clr)
    if msg.subagent_label:
        left.append(f"  [{msg.subagent_label}]", style=theme.ITEM_AGENT)
    if is_last and ongoing:
        left.append(f"  {theme.SPIN[anim_frame]}", style=theme.ONGOING)
    left.append(f"  {idx + 1}/{total}", style=theme.TEXT_DIM)

    tbl.add_row(Text("▌ ", style=f"bold {border_clr}"), left, _stats_right(msg))

    # ── Content preview row ────────────────────────────────────────────────────
    content_preview = truncate(first_line(msg.content), 300)
    tbl.add_row(
        Text("│ ", style=theme.TEXT_MUTED),
        Text(content_preview, style=theme.TEXT_DIM),
        Text(""),
    )

    # ── Expanded item rows ────────────────────────────────────────────────────
    if is_expanded and msg.items:
        n_items = len(msg.items)
        for item_idx, item in enumerate(msg.items):
            has_agent = len(item.subagent_messages) > 0
            is_last_item = item_idx == n_items - 1
            branch = "└─" if is_last_item else "├─"

            if item.team_color:
                bar_clr = get_team_color(item.team_color)
                item_clr = get_team_color(item.team_color)
            else:
                bar_clr = theme.BORDER
                item_clr = get_item_color(item.item_type, bool(item.tool_error))

            icon_str = get_item_icon(item)
            icon_str = icon_str + " " * (2 - cell_len(icon_str))
            name_str = get_item_name(item)
            extras = ""
            if item.tool_summary:
                extras = f" — {item.tool_summary}"
            elif item.subagent_desc:
                extras = f" — {item.subagent_desc}"
            elif item.text and item.item_type not in ("ToolCall",):
                extras = f" {item.text}"

            item_left = Text()
            item_left.append(f"  {branch} ", style=bar_clr)
            item_left.append(f"{icon_str} {name_str}", style=item_clr)
            if extras:
                item_left.append(extras, style=theme.TEXT_DIM)
            if item.subagent_ongoing:
                item_left.append(f" {theme.SPIN[anim_frame]}", style=theme.ONGOING)

            item_right = Text()
            r_parts: list[tuple[str, str]] = []
            if item.token_count > 0:
                r_parts.append((format_tokens(item.token_count), theme.TEXT_DIM))
            if item.duration_ms > 0:
                r_parts.append((format_duration(item.duration_ms), theme.TEXT_DIM))
            if has_agent:
                clr = get_team_color(item.team_color) if item.team_color else theme.ITEM_AGENT
                r_parts.append((f"[{len(item.subagent_messages)} msg]", clr))
            for j, (t, s) in enumerate(r_parts):
                if j > 0:
                    item_right.append(" · ", style=theme.TEXT_MUTED)
                item_right.append(t, style=s)

            tbl.add_row(
                Text("│ ", style=theme.TEXT_MUTED),
                item_left,
                item_right,
            )

    return tbl


class MessageList(HighlightListView):
    """Full-width session message list with a spinning progress indicator.

    Inherits the shared highlight CSS + auto-index from HighlightListView.
    """

    BINDINGS = [
        *HighlightListView.BINDINGS,
        Binding("t", "show_teams", "Teams", show=True),
        Binding("s", "open_picker", "Sessions", show=True),
    ]

    DEFAULT_CSS = """
    MessageList {
        height: 100%;
    }
    MessageList ListItem {
        padding: 1 1;
    }
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._messages: list[DisplayMessage] = []
        self._expanded_set: set[int] = set()
        self._ongoing: bool = False
        self._anim_frame: int = 0

    def on_mount(self) -> None:
        self.set_interval(0.5, self._spin)

    def _spin(self) -> None:
        """Advance spinner frame and re-render affected cards."""
        try:
            if not (self._ongoing and self._messages):
                return
            self._anim_frame = (self._anim_frame + 1) % len(theme.SPIN)
            # Re-render last message (carries the main ongoing indicator)
            self.refresh_item(len(self._messages) - 1)
            # Re-render any expanded messages that have ongoing subagents
            for idx in self._expanded_set:
                if idx < len(self._messages):
                    msg = self._messages[idx]
                    if any(it.subagent_ongoing for it in (msg.items or [])):
                        self.refresh_item(idx)
        except Exception:
            pass

    # ----------------------------------------------------------------
    # Public API
    # ----------------------------------------------------------------

    async def populate(
        self,
        messages: list[DisplayMessage],
        expanded_set: set[int],
        ongoing: bool,
    ) -> None:
        """Sync the list to the new state.

        Must be async + awaited: ListView.clear() / append() / remove() all
        return AwaitComplete and the mutations are not synchronously
        reflected in `self._nodes`. A sync populate that fires twice in
        quick succession (e.g. self.messages=[] then =real) races with its
        own pending clears, producing duplicated rows and an index that
        snaps back to None when the deferred clear finally lands.
        """
        old_messages = self._messages
        old_expanded = self._expanded_set
        old_ongoing = self._ongoing

        new_total = len(messages)
        old_total = len(old_messages)

        self._messages = messages
        self._expanded_set = expanded_set
        self._ongoing = ongoing

        if new_total == 0:
            await self.clear()
            await self.append(
                ListItem(
                    Static(Text("No messages loaded", style=theme.TEXT_DIM)),
                    disabled=True,
                )
            )
            return

        node_count = len(self._nodes)
        if old_total == 0 or node_count != old_total:
            await self.clear()
            for idx, msg in enumerate(messages):
                is_expanded = idx in expanded_set
                is_last = idx == new_total - 1
                renderable = _render_message(
                    msg,
                    idx,
                    new_total,
                    is_expanded,
                    is_last,
                    ongoing,
                    anim_frame=self._anim_frame,
                )
                await self.append(ListItem(Static(renderable)))
            self.ensure_highlight()
            return

        # Incremental diff path
        overlap = min(old_total, new_total)
        # Length change shifts every row's "X/Y" footer; ongoing flip moves
        # the spinner — both invalidate all overlap rows.
        refresh_all_overlap = old_total != new_total or old_ongoing != ongoing

        for idx in range(overlap):
            if (
                refresh_all_overlap
                or messages[idx] != old_messages[idx]
                or (idx in expanded_set) != (idx in old_expanded)
            ):
                self.refresh_item(idx)

        for idx in range(old_total, new_total):
            is_expanded = idx in expanded_set
            is_last = idx == new_total - 1
            renderable = _render_message(
                messages[idx],
                idx,
                new_total,
                is_expanded,
                is_last,
                ongoing,
                anim_frame=self._anim_frame,
            )
            await self.append(ListItem(Static(renderable)))

        if new_total < old_total:
            for _ in range(old_total - new_total):
                with contextlib.suppress(Exception):
                    await self._nodes[-1].remove()

        self.ensure_highlight()

    def refresh_item(self, idx: int) -> None:
        """Re-render a single item in place (after toggle-expand or spinner tick)."""
        if idx < 0 or idx >= len(self._messages):
            return
        msg = self._messages[idx]
        total = len(self._messages)
        is_expanded = idx in self._expanded_set
        is_last = idx == total - 1
        renderable = _render_message(
            msg,
            idx,
            total,
            is_expanded,
            is_last,
            self._ongoing,
            anim_frame=self._anim_frame,
        )
        try:
            item = self._nodes[idx]
            static = item.query_one(Static)
            static.update(renderable)
        except Exception:
            pass
