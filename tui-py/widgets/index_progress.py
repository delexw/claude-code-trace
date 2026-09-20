"""IndexProgressBar widget — how much of the projects directory the backend has read."""

from __future__ import annotations

from rich.text import Text
from textual.reactive import reactive
from textual.widgets import Static

import theme
from data_types import IndexProgress
from format_utils import format_bytes, progress_bar

BAR_WIDTH = 20


def index_progress_line(progress: IndexProgress | None) -> Text:
    """One line of scan progress; empty before a scan starts and once it is done.

    Measured in bytes rather than files: one huge session among three
    thousand small ones would otherwise sit at 99% with nearly all the
    reading still to do. The label counts bytes too, so the number and the
    bar cannot contradict each other.
    """
    if progress is None or progress.done:
        return Text("")
    if progress.total_bytes <= 0:
        return Text("Counting sessions…", style=theme.TEXT_DIM)

    percent = min(100, round(progress.bytes_read * 100 / progress.total_bytes))
    line = Text()
    line.append(progress_bar(percent, BAR_WIDTH), style=theme.ACCENT)
    line.append(f" {percent}%", style=f"bold {theme.ACCENT}")
    line.append(
        f"  {progress.files_read:,} sessions · "
        f"{format_bytes(progress.bytes_read)} / {format_bytes(progress.total_bytes)}",
        style=theme.TEXT_DIM,
    )
    return line


class IndexProgressBar(Static):
    """Strip above the footer showing the backend's session scan.

    Keeps its row whether a scan is running or not: a strip that came and
    went would shift everything above it each time a scan starts.
    """

    DEFAULT_CSS = """
    IndexProgressBar {
        height: 1;
        padding: 0 1;
    }
    """

    progress: reactive[IndexProgress | None] = reactive(None)

    def watch_progress(self, _p: IndexProgress | None) -> None:
        self.update(index_progress_line(self.progress))
