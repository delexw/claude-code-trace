"""DebugViewer widget — DataTable of debug log entries."""

from __future__ import annotations

from textual.binding import Binding
from textual.widgets import DataTable

import theme
from data_types import DebugEntry


def _level_color(level: str) -> str:
    match level.lower():
        case "error":
            return theme.ERROR
        case "warn":
            return theme.TOKEN_HIGH
        case "info":
            return theme.ACCENT
        case "debug":
            return theme.TEXT_DIM
        case _:
            return theme.TEXT_PRIMARY


def _format_ts(timestamp: str) -> str:
    """Extract HH:MM:SS.mmm from an ISO timestamp."""
    if not timestamp:
        return ""
    t_part = timestamp.split("T")[-1].replace("Z", "")
    return t_part[:12] if len(t_part) >= 12 else t_part


class DebugViewer(DataTable):
    """DataTable showing debug log entries.

    Columns: Time | Level | Category | Message
    j/k and up/down navigation are handled natively.
    """

    BINDINGS = [
        *DataTable.BINDINGS,
        Binding("j", "cursor_down", "Down", show=False),
        Binding("k", "cursor_up", "Up", show=False),
    ]

    DEFAULT_CSS = """
    DebugViewer {
        height: 100%;
    }
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._entries: list[DebugEntry] = []

    def on_mount(self) -> None:
        self.add_columns("Time", "Level", "Category", "Message")
        self.cursor_type = "row"
        self.show_header = True

    def populate(self, entries: list[DebugEntry]) -> None:
        """Rebuild the table with new entries."""
        self._entries = entries
        self.clear()
        for e in entries:
            ts = _format_ts(e.timestamp)
            level = e.level.upper()
            category = e.category or ""
            msg = e.message
            if e.count > 1:
                msg += f" (x{e.count})"
            self.add_row(ts, level, category, msg)

    def entries(self) -> list[DebugEntry]:
        return self._entries
