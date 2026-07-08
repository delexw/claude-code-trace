"""Tests for CCTraceApp's copy_resume binding — copies `claude --resume …`
for the currently selected session, and is only relevant (visible) once a
session is in context (list/detail views), not in the picker's global footer,
which already overflows on narrow terminals (see check_action)."""

from __future__ import annotations

import pytest

import api
import app as app_module
from data_types import SessionInfo


class _NoopSSE:
    """Stands in for SSEClient so App.on_mount doesn't open a real connection."""

    def __init__(self, *_args, **_kwargs):
        pass

    def on(self, *_args, **_kwargs):
        pass

    def start(self):
        pass

    def stop(self):
        pass


async def _fake_get_project_dirs():
    return []


@pytest.fixture(autouse=True)
def _stub_network(monkeypatch):
    """Keep App.on_mount's background workers from touching a real backend."""
    monkeypatch.setattr(app_module, "SSEClient", _NoopSSE)
    monkeypatch.setattr(api, "get_project_dirs", _fake_get_project_dirs)


@pytest.mark.asyncio
async def test_action_copy_resume_copies_the_resume_command(monkeypatch):
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.view = "list"
        app._current_session = SessionInfo(cwd="/it's", session_id="u1")

        copied: list[str] = []
        monkeypatch.setattr(app, "copy_to_clipboard", copied.append)

        app.action_copy_resume()

        assert copied == ["cd '/it'\\''s' && claude --resume u1"]


@pytest.mark.asyncio
async def test_action_copy_resume_is_noop_without_a_selected_session():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.view = "list"
        app._current_session = None

        # Must not raise — there's nothing to copy.
        app.action_copy_resume()


@pytest.mark.asyncio
async def test_copy_resume_hidden_in_picker_view():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.view = "picker"
        app._current_session = None

        assert app.check_action("copy_resume", ()) is False


@pytest.mark.asyncio
async def test_copy_resume_visible_in_list_view_with_a_selected_session():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.view = "list"
        app._current_session = SessionInfo(cwd="/x", session_id="u2")

        assert app.check_action("copy_resume", ()) is True


@pytest.mark.asyncio
async def test_y_key_copies_resume_command_via_pilot(monkeypatch):
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.view = "detail"
        app._current_session = SessionInfo(cwd="/x", session_id="u3")

        copied: list[str] = []
        monkeypatch.setattr(app, "copy_to_clipboard", copied.append)

        await pilot.press("y")

        assert copied == ["cd '/x' && claude --resume u3"]
