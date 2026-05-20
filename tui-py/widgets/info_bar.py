"""InfoBar widget — shows session metadata at the top of the screen."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Static

from data_types import DisplayMessage, SessionMeta, SessionTotals
from format_utils import (
    context_percent,
    format_cost,
    format_tokens,
    short_mode,
    short_path,
)
from theme import (
    ACCENT,
    GIT_BRANCH,
    TEXT_DIM,
    TEXT_MUTED,
    TOKEN_HIGH,
    get_context_color,
    mode_color,
)


class InfoBar(Widget):
    """Top bar showing project name, git branch, permission mode, token usage."""

    DEFAULT_CSS = """
    InfoBar {
        height: 3;
        border: round $border;
        padding: 0 2;
        layout: horizontal;
    }
    """

    meta: reactive[SessionMeta] = reactive(SessionMeta, recompose=True)
    messages: reactive[list[DisplayMessage]] = reactive(list, recompose=True)
    session_totals: reactive[SessionTotals] = reactive(SessionTotals, recompose=True)
    session_path: reactive[str] = reactive("", recompose=True)
    ongoing: reactive[bool] = reactive(False, recompose=True)

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.meta = SessionMeta()
        self.messages = []
        self.session_totals = SessionTotals()
        self.session_path = ""
        self.ongoing = False

    def render(self) -> str:
        meta = self.meta
        messages = self.messages
        totals = self.session_totals
        session_path = self.session_path

        project_name = short_path(meta.cwd)
        session_id = session_path.split("/")[-1].replace(".jsonl", "") if session_path else ""
        branch = meta.git_branch
        mode = meta.permission_mode

        ctx_pct = context_percent(messages)

        dot = f"[{TEXT_MUTED}] · [/]"

        parts_left = []
        if project_name:
            parts_left.append(f"[bold {ACCENT}]{project_name}[/]")
        if session_id:
            parts_left.append(f"{dot}[{TEXT_DIM}]{session_id[:8]}[/]")
        if branch:
            parts_left.append(f"{dot}[{GIT_BRANCH}]* {branch}[/]")
        if mode and mode != "default":
            clr = mode_color(mode)
            parts_left.append(f"{dot}[bold {clr}]{short_mode(mode)}[/]")

        parts_right = []
        if ctx_pct >= 0:
            clr = get_context_color(ctx_pct)
            parts_right.append(f"[{clr}]ctx {ctx_pct}%[/]")
        if totals.total_tokens > 0:
            parts_right.append(f"{dot}[{TEXT_DIM}]{format_tokens(totals.total_tokens)} tok[/]")
        if totals.cost_usd > 0:
            parts_right.append(f"{dot}[{TOKEN_HIGH}]{format_cost(totals.cost_usd)}[/]")
        if self.ongoing:
            parts_right.append(f"[{ACCENT}] ●[/]")

        left = "".join(parts_left)
        right = "".join(parts_right)

        # Pad between left and right
        return f"{left}{{:>{self.size.width - self._markup_len(left) - self._markup_len(right) - 4}s}}{right}".format(
            ""
        )

    def _markup_len(self, s: str) -> int:
        """Approximate visible length of a Rich markup string."""
        import re

        return len(re.sub(r"\[.*?\]", "", s))

    def compose(self) -> ComposeResult:
        yield Static(self._build_content(), id="infobar-content")

    def _build_content(self) -> str:
        meta = self.meta
        messages = self.messages
        totals = self.session_totals
        session_path = self.session_path

        project_name = short_path(meta.cwd)
        session_id = session_path.split("/")[-1].replace(".jsonl", "") if session_path else ""
        branch = meta.git_branch
        mode = meta.permission_mode

        ctx_pct = context_percent(messages)
        dot = f"[{TEXT_MUTED}] · [/]"

        parts_left = []
        if project_name:
            parts_left.append(f"[bold {ACCENT}]{project_name}[/]")
        if session_id:
            parts_left.append(f"{dot}[{TEXT_DIM}]{session_id[:8]}[/]")
        if branch:
            parts_left.append(f"{dot}[{GIT_BRANCH}]* {branch}[/]")
        if mode and mode != "default":
            clr = mode_color(mode)
            parts_left.append(f"{dot}[bold {clr}]{short_mode(mode)}[/]")

        parts_right = []
        if ctx_pct >= 0:
            clr = get_context_color(ctx_pct)
            parts_right.append(f"[{clr}]ctx {ctx_pct}%[/]")
        if totals.total_tokens > 0:
            parts_right.append(f"{dot}[{TEXT_DIM}]{format_tokens(totals.total_tokens)} tok[/]")
        if totals.cost_usd > 0:
            parts_right.append(f"{dot}[{TOKEN_HIGH}]{format_cost(totals.cost_usd)}[/]")
        if self.ongoing:
            parts_right.append(f" [{ACCENT}]●[/]")

        left = "".join(parts_left)
        right = "".join(parts_right)
        return left + "   " + right

    def on_mount(self) -> None:
        self._refresh_content()

    def watch_meta(self, _meta) -> None:
        self._refresh_content()

    def watch_messages(self, _msgs) -> None:
        self._refresh_content()

    def watch_session_totals(self, _tot) -> None:
        self._refresh_content()

    def watch_session_path(self, _p) -> None:
        self._refresh_content()

    def watch_ongoing(self, _o) -> None:
        self._refresh_content()

    def _refresh_content(self) -> None:
        try:
            static = self.query_one("#infobar-content", Static)
            static.update(self._build_content())
        except Exception:
            pass
