"""KeybindBar widget — shows keyboard hints at the bottom of the screen."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Static

from theme import ACCENT, TEXT_DIM, TEXT_MUTED

PICKER_KEYS = [
    ("h", "sidebar"),
    ("j/k", "nav"),
    ("g/G", "jump"),
    ("Enter", "open"),
    ("r", "refresh"),
    ("q", "quit"),
]

LIST_KEYS = [
    ("h", "sidebar"),
    ("j/k", "nav"),
    ("G/g", "jump"),
    ("Tab/↵", "detail"),
    ("e/c", "expand"),
    ("d", "debug"),
    ("q", "sessions"),
]

LIST_KEYS_WITH_TEAMS = LIST_KEYS[:5] + [("t", "tasks")] + LIST_KEYS[5:]

DETAIL_KEYS = [
    ("j/k", "items"),
    ("Enter/Tab", "toggle"),
    ("u/d", "scroll"),
    ("e/c", "expand"),
    ("q/Esc", "back"),
]

DEBUG_KEYS = [("q/Esc", "back")]
TEAM_KEYS = [("q/Esc", "back")]


def get_keys(view: str, has_teams: bool) -> list[tuple[str, str]]:
    if view == "picker":
        return PICKER_KEYS
    if view == "list":
        return LIST_KEYS_WITH_TEAMS if has_teams else LIST_KEYS
    if view == "detail":
        return DETAIL_KEYS
    if view == "debug":
        return DEBUG_KEYS
    if view == "team":
        return TEAM_KEYS
    return []


class KeybindBar(Widget):
    """Bottom bar showing keyboard hints."""

    DEFAULT_CSS = """
    KeybindBar {
        height: 3;
        border: round $border;
        padding: 0 1;
    }
    """

    view: reactive[str] = reactive("picker")
    has_teams: reactive[bool] = reactive(False)
    position: reactive[str] = reactive("")

    def compose(self) -> ComposeResult:
        yield Static(self._build_content(), id="keybind-content")

    def _build_content(self) -> str:
        keys = get_keys(self.view, self.has_teams)
        dot = f"[{TEXT_MUTED}] · [/]"
        parts = [f"[{TEXT_DIM}]cctrace[/]"]
        for key, label in keys:
            parts.append(f"{dot}[bold {ACCENT}]{key}[/] [{TEXT_DIM}]{label}[/]")
        left = "".join(parts)
        right = f"[{TEXT_DIM}]{self.position}[/]" if self.position else ""
        sep = "   " if right else ""
        return left + sep + right

    def watch_view(self, _v) -> None:
        self._refresh_content()

    def watch_has_teams(self, _t) -> None:
        self._refresh_content()

    def watch_position(self, _p) -> None:
        self._refresh_content()

    def _refresh_content(self) -> None:
        try:
            static = self.query_one("#keybind-content", Static)
            static.update(self._build_content())
        except Exception:
            pass

    def on_mount(self) -> None:
        self._refresh_content()
