"""SessionPicker widget — ListView of sessions grouped by date."""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

from rich.console import Group
from rich.rule import Rule
from rich.text import Text
from textual.widgets import ListItem, Static

import theme
from data_types import SessionInfo
from format_utils import format_cost, format_tokens, short_model, time_ago
from theme import get_model_color
from widgets.highlight_list import HighlightListView

# Icons
ICON_SELECTED = "▸"
ICON_BRANCH = "*"
ICON_CHAT = "#"
ICON_CLOCK = "~"
ICON_SESSION = "@"


def _group_by_date(sessions: list[SessionInfo]) -> list[tuple[str, list[SessionInfo]]]:
    now = datetime.now(UTC)
    today_start = now.replace(hour=0, minute=0, second=0, microsecond=0)
    yesterday_start = today_start - timedelta(days=1)
    week_start = today_start - timedelta(days=7)
    month_start = today_start - timedelta(days=30)

    groups: dict[str, list[SessionInfo]] = {}
    order = ["Today", "Yesterday", "This Week", "This Month", "Older"]

    for s in sessions:
        try:
            d = datetime.fromisoformat(s.mod_time.replace("Z", "+00:00"))
        except Exception:
            d = now
        if d >= today_start:
            cat = "Today"
        elif d >= yesterday_start:
            cat = "Yesterday"
        elif d >= week_start:
            cat = "This Week"
        elif d >= month_start:
            cat = "This Month"
        else:
            cat = "Older"
        groups.setdefault(cat, []).append(s)

    for cat in order:
        if cat in groups:
            groups[cat].sort(key=lambda s: s.mod_time, reverse=True)

    return [(cat, groups[cat]) for cat in order if cat in groups and groups[cat]]


def _render_session(s: SessionInfo, anim_frame: int = 0) -> object:
    """Render a session as a Rich Group with full-width separator."""
    model_str = short_model(s.model) if s.model else ""
    model_clr = get_model_color(s.model) if s.model else theme.TEXT_DIM

    # Line 1: title. Prefer the user-assigned /rename name (accent-highlighted);
    # otherwise use the first message. When a name is shown, the first message is
    # kept as the subtitle.
    title = s.name or s.first_message or s.session_id
    line1 = Text()
    line1.append(title, style=theme.ACCENT if s.name else theme.TEXT_PRIMARY)
    if s.is_ongoing:
        line1.append(f"  {theme.SPIN[anim_frame]}", style=theme.ONGOING)

    # Line 2: metadata
    line2 = Text()
    if model_str:
        line2.append(model_str, style=model_clr)
    if s.git_branch:
        line2.append(f" {ICON_BRANCH} {s.git_branch}", style=theme.GIT_BRANCH)
    line2.append(f" {ICON_CHAT} {s.turn_count}", style=theme.TEXT_DIM)
    if s.total_tokens > 0:
        tok_clr = theme.TOKEN_HIGH if s.total_tokens > 150000 else theme.TEXT_DIM
        line2.append(f" {format_tokens(s.total_tokens)} tok", style=tok_clr)
    if s.cost_usd > 0:
        line2.append(f" {format_cost(s.cost_usd)}", style=theme.TOKEN_HIGH)
    line2.append(f" {ICON_SESSION} {s.session_id[:8]}", style=theme.TEXT_DIM)
    line2.append(f" {ICON_CLOCK} {time_ago(s.mod_time)}", style=theme.TEXT_DIM)

    if s.name and s.first_message:
        subtitle = Text(s.first_message, style=theme.TEXT_DIM)
        content = Text.assemble(line1, "\n", subtitle, "\n", line2)
    else:
        content = Text.assemble(line1, "\n", line2)
    sep = Rule(style=theme.TEXT_MUTED, characters="─")
    return Group(content, sep)


class SessionPicker(HighlightListView):
    """Session list grouped by date.

    Inherits the shared highlight CSS + auto-index from HighlightListView so
    selection styling stays in sync with MessageList and the DetailView items.
    """

    DEFAULT_CSS = """
    SessionPicker {
        height: 100%;
    }
    SessionPicker ListItem {
        padding: 1 2;
    }
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._index_to_session: dict[int, SessionInfo] = {}
        self._loading: bool = True
        self._error: str = ""
        self._anim_frame: int = 0

    def on_mount(self) -> None:
        self.set_interval(0.5, self._spin)

    def _spin(self) -> None:
        """Advance spinner and re-render ongoing session rows."""
        try:
            if not any(s.is_ongoing for s in self._index_to_session.values()):
                return
            self._anim_frame = (self._anim_frame + 1) % len(theme.SPIN)
            for raw_idx, session in self._index_to_session.items():
                if session.is_ongoing:
                    self._refresh_item(raw_idx, session)
        except Exception:
            pass

    def _refresh_item(self, raw_idx: int, session: SessionInfo) -> None:
        try:
            item = self._nodes[raw_idx]
            static = item.query_one(Static)
            static.update(_render_session(session, self._anim_frame))
        except Exception:
            pass

    # ----------------------------------------------------------------
    # Public API
    # ----------------------------------------------------------------

    def populate(
        self,
        sessions: list[SessionInfo],
        loading: bool,
        error: str,
    ) -> None:
        """Rebuild the list contents."""
        self._loading = loading
        self._error = error
        self._index_to_session = {}

        # Eagerly clear so neither stale sessions nor the previous overlay
        # sit under the new state.
        self.clear()
        self.loading = loading

        if loading:
            return

        if error:
            self.append(ListItem(Static(f"[{theme.ERROR}]{error}[/]")))
            return

        if not sessions:
            self.append(ListItem(Static(f"[{theme.TEXT_DIM}]No sessions found[/]")))
            return

        raw_idx = 0

        # Header (disabled)
        total_tokens = sum(s.total_tokens for s in sessions)
        total_cost = sum(s.cost_usd for s in sessions)
        header_parts = [f"[bold]Sessions ({len(sessions)})[/]"]
        if total_tokens > 0:
            header_parts.append(f"  [{theme.TEXT_DIM}]{format_tokens(total_tokens)} tok[/]")
        if total_cost > 0:
            header_parts.append(f"  [{theme.TOKEN_HIGH}]{format_cost(total_cost)}[/]")
        self.append(ListItem(Static("".join(header_parts)), disabled=True))
        raw_idx += 1

        date_groups = _group_by_date(sessions)

        for cat, group_sessions in date_groups:
            # Date group header (non-selectable)
            self.append(ListItem(Static(f"[bold {theme.TEXT_DIM}]{cat}[/]"), disabled=True))
            raw_idx += 1

            for s in group_sessions:
                renderable = _render_session(s, self._anim_frame)
                self.append(ListItem(Static(renderable)))
                self._index_to_session[raw_idx] = s
                raw_idx += 1

        # Pick the first selectable row (HighlightListView skips disabled headers).
        self.ensure_highlight()

    def session_at_raw_index(self, raw_idx: int) -> SessionInfo | None:
        """Return the SessionInfo at the given raw ListView node index.

        Header rows (session header, date group headers) are not in the map and return None.
        """
        return self._index_to_session.get(raw_idx)
