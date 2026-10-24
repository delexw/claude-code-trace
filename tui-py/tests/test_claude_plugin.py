"""Tests for the Claude plugin's picker wiring inside the app shell."""

from __future__ import annotations

import pytest

import api
import app as app_module
from data_types import SessionInfo
from format_utils import project_key
from widgets.project_tree import ProjectTree
from widgets.source_picker import SourcePicker

ONE = SessionInfo(
    session_id="sid-one000001",
    path="/home/u/.claude/projects/-proj-one/one.jsonl",
    mod_time="2026-09-20T10:00:00Z",
)
TWO = SessionInfo(
    session_id="sid-two000002",
    path="/home/u/.claude/projects/-proj-two/two.jsonl",
    mod_time="2026-09-20T09:00:00Z",
)


class _NoopSSE:
    def __init__(self, *_args, **_kwargs):
        pass

    def on(self, *_args, **_kwargs):
        pass

    def start(self):
        pass

    def stop(self):
        pass


@pytest.fixture(autouse=True)
def _stub_network(monkeypatch):
    monkeypatch.setattr(app_module, "SSEClient", _NoopSSE)

    async def dirs():
        return ["/p"]

    async def discover(_dirs):
        return [ONE, TWO]

    async def nothing(*_args, **_kwargs):
        return None

    async def empty():
        return []

    async def no_settings():
        raise RuntimeError("no backend")

    monkeypatch.setattr(api, "get_project_dirs", dirs)
    monkeypatch.setattr(api, "discover_sessions", discover)
    monkeypatch.setattr(api, "watch_picker", nothing)
    monkeypatch.setattr(api, "list_efficiency_jobs", empty)
    monkeypatch.setattr(api, "list_efficiency_summaries", empty)
    monkeypatch.setattr(api, "get_settings", no_settings)


def _session_ids(app) -> list[str]:
    picker = app.query_one("#picker", SourcePicker)
    return [picker.entry_at_raw_index(i)[1].session_id for i in picker.selectable_indices()]


@pytest.mark.asyncio
async def test_an_unchanged_refresh_keeps_the_picker_cursor():
    """picker-refresh arrives on every file change; rebuilding the list for an
    identical result would throw the user's cursor back to the top."""
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await app.workers.wait_for_complete()
        await pilot.pause()
        picker = app.query_one("#picker", SourcePicker)
        picker.index = picker.selectable_indices()[-1]
        nodes_before = list(picker._nodes)

        await app.plugins.get("claude").on_picker_refresh({})
        await pilot.pause()

        assert picker.index == picker.selectable_indices()[-1]
        assert list(picker._nodes) == nodes_before


@pytest.mark.asyncio
async def test_choosing_a_project_in_the_sidebar_filters_the_picker():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await app.workers.wait_for_complete()
        await pilot.pause()
        assert _session_ids(app) == ["sid-one000001", "sid-two000002"]

        app.query_one("#project-tree", ProjectTree).post_message(
            ProjectTree.ProjectSelected(project_key(TWO.path))
        )
        await pilot.pause()

        assert _session_ids(app) == ["sid-two000002"]
        assert app.view == "picker"
