"""Tests for the session-scan progress strip."""

from __future__ import annotations

import pytest

from data_types import IndexProgress, index_progress_from_dict
from widgets.index_progress import IndexProgressBar, index_progress_line


def test_index_progress_from_dict_reads_the_sse_payload():
    progress = index_progress_from_dict(
        {
            "files_read": 12,
            "total_files": 40,
            "bytes_read": 2_000_000,
            "total_bytes": 8_000_000,
            "done": False,
        }
    )
    assert progress.files_read == 12
    assert progress.bytes_read == 2_000_000
    assert progress.done is False


def test_no_line_before_a_scan_starts_or_once_it_is_done():
    assert index_progress_line(None).plain == ""
    assert index_progress_line(IndexProgress(done=True)).plain == ""


def test_counting_phase_says_so_rather_than_showing_a_zero_bar():
    line = index_progress_line(IndexProgress(total_bytes=0))
    assert line.plain == "Counting sessions…"


def test_percentage_is_measured_in_bytes_not_files():
    """One huge session among many small ones must not read as nearly done."""
    line = index_progress_line(
        IndexProgress(files_read=99, total_files=100, bytes_read=1_000_000, total_bytes=10_000_000)
    )
    assert " 10%" in line.plain
    assert "99 sessions" in line.plain
    assert "1.0 MB / 10.0 MB" in line.plain


@pytest.mark.asyncio
async def test_widget_renders_the_current_progress():
    from textual.app import App, ComposeResult

    class Harness(App):
        def compose(self) -> ComposeResult:
            yield IndexProgressBar(id="index-progress")

    async with Harness().run_test() as pilot:
        bar = pilot.app.query_one("#index-progress", IndexProgressBar)
        bar.progress = IndexProgress(files_read=3, bytes_read=500, total_bytes=1_000)
        await pilot.pause()
        assert "50%" in bar.content.plain

        bar.progress = IndexProgress(done=True)
        await pilot.pause()
        assert bar.content.plain == ""
