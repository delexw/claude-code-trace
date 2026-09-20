"""Tests for the Jev line the session picker adds to a row: the score, a
running analysis, and a failure."""

from __future__ import annotations

import pytest
from textual.widgets import ListItem, Static

import theme
from data_types import SessionInfo
from efficiency import EfficiencyJob, EfficiencySummary
from widgets.session_picker import SessionPicker, _analysis_line, _render_session


def _plain(session: SessionInfo, job=None, summary=None) -> str:
    group = _render_session(session, job=job, summary=summary)
    return group.renderables[0].plain


def test_no_analysis_line_for_a_session_that_was_never_analysed():
    assert _analysis_line(None, None) is None
    assert "Jev" not in _plain(SessionInfo(session_id="sid-plain0001"))


def test_completed_analysis_shows_the_score():
    summary = EfficiencySummary(session_path="/p/s.jsonl", score=82)
    line = _analysis_line(None, summary)
    assert line is not None
    assert "Jev 82" in line.plain
    assert "stale" not in line.plain


def test_stale_score_says_so():
    summary = EfficiencySummary(session_path="/p/s.jsonl", score=82, stale=True)
    assert "stale" in _analysis_line(None, summary).plain


def test_running_analysis_shows_a_bar_and_its_phase():
    job = EfficiencyJob(status="sending", progress=45, message="Sending to Jev")
    line = _analysis_line(job, None)
    assert "Sending" in line.plain
    assert "45%" in line.plain
    assert "█" in line.plain
    assert "Sending to Jev" in line.plain


def test_running_analysis_with_no_progress_yet_reads_as_zero():
    job = EfficiencyJob(status="queued", progress=None)
    assert "0%" in _analysis_line(job, None).plain


def test_a_rerun_shows_the_old_score_next_to_the_running_bar():
    job = EfficiencyJob(status="analysing", progress=60)
    summary = EfficiencySummary(session_path="/p/s.jsonl", score=70)
    line = _analysis_line(job, summary)
    assert "Jev 70" in line.plain
    assert "60%" in line.plain


def test_failed_analysis_surfaces_the_error():
    job = EfficiencyJob(status="failed", error="Jev rejected the API key")
    line = _analysis_line(job, None)
    assert "Analysis failed" in line.plain
    assert "Jev rejected the API key" in line.plain
    assert str(theme.ERROR) in str(line.spans[0].style)


def test_a_finished_job_next_to_a_score_does_not_repeat_the_failure():
    """Once a later run succeeded, the score is the story — not the old failure."""
    job = EfficiencyJob(status="failed")
    summary = EfficiencySummary(session_path="/p/s.jsonl", score=88)
    line = _analysis_line(job, summary)
    assert "Analysis failed" not in line.plain
    assert "Jev 88" in line.plain


def test_render_session_appends_the_analysis_line():
    session = SessionInfo(session_id="sid-score0001", path="/p/s.jsonl")
    summary = EfficiencySummary(session_path="/p/s.jsonl", score=91)
    assert "Jev 91" in _plain(session, summary=summary)


@pytest.mark.asyncio
async def test_update_analysis_refreshes_rows_without_rebuilding_the_list():
    """Progress arrives every second or so — repopulating would clear the list
    and drop the cursor each time."""
    from textual.app import App, ComposeResult

    class Harness(App):
        def compose(self) -> ComposeResult:
            yield SessionPicker(id="picker")

    sessions = [
        SessionInfo(
            session_id="sid-one000001", path="/p/one.jsonl", mod_time="2026-09-20T10:00:00Z"
        ),
        SessionInfo(
            session_id="sid-two000002", path="/p/two.jsonl", mod_time="2026-09-20T09:00:00Z"
        ),
    ]

    async with Harness().run_test() as pilot:
        picker = pilot.app.query_one("#picker", SessionPicker)
        picker.populate(sessions=sessions, loading=False, error="")
        await pilot.pause()

        picker.index = picker._first_selectable_index() + 1
        selected_before = picker.index
        nodes_before = list(picker._nodes)

        job = EfficiencyJob(session_path="/p/one.jsonl", status="analysing", progress=75)
        picker.update_analysis({"/p/one.jsonl": job}, {})
        await pilot.pause()

        assert picker.index == selected_before
        assert list(picker._nodes) == nodes_before

        def row_text(raw_idx: int) -> str:
            item: ListItem = picker._nodes[raw_idx]
            return item.query_one(Static).content.renderables[0].plain

        analysed_idx = next(
            idx for idx, s in picker._index_to_session.items() if s.path == "/p/one.jsonl"
        )
        untouched_idx = next(
            idx for idx, s in picker._index_to_session.items() if s.path == "/p/two.jsonl"
        )
        assert "75%" in row_text(analysed_idx)
        assert "Jev" not in row_text(untouched_idx)
