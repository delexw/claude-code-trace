"""Tests for CCTraceApp._on_session_update — the session-update SSE handler.

The backend's `session-update` event was changed to a lightweight refresh
signal (message count + roles only, never message bodies) — see
`SessionUpdatePayload` in src-tauri/src/watcher.rs. `_on_session_update` must
re-fetch the full session via `load_session` rather than trusting a
`messages` key that no longer exists in the payload, or every live update
while a session is running silently empties `self.messages`.
"""

from __future__ import annotations

import pytest

import api
import app as app_module
from data_types import DisplayMessage, LoadResult, SessionMeta, SessionTotals


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
async def test_on_session_update_refetches_full_session(monkeypatch):
    fake_messages = [DisplayMessage(role="claude", content="hello")]
    fake_result = LoadResult(
        messages=fake_messages,
        teams=[],
        ongoing=True,
        meta=SessionMeta(cwd="/proj", git_branch="main", permission_mode="default"),
        session_totals=SessionTotals(total_tokens=42),
    )

    async def fake_load_session(path):
        assert path == "/proj/session.jsonl"
        return fake_result

    monkeypatch.setattr(api, "load_session", fake_load_session)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.session_path = "/proj/session.jsonl"
        app.messages = []

        # The real payload only carries count/roles, never message bodies —
        # the handler must ignore that and re-fetch instead of trusting it.
        await app._on_session_update({"count": 1, "roles": ["claude"]})

        assert app.messages == fake_messages
        assert app.ongoing is True
        assert app.totals.total_tokens == 42


@pytest.mark.asyncio
async def test_on_session_update_is_noop_without_a_session_path(monkeypatch):
    async def fail_load_session(_path):
        raise AssertionError("load_session should not be called with no session open")

    monkeypatch.setattr(api, "load_session", fail_load_session)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.session_path = ""

        await app._on_session_update({})


@pytest.mark.asyncio
async def test_on_session_update_keeps_prior_messages_when_refetch_fails(monkeypatch):
    prior_messages = [DisplayMessage(role="user", content="still here")]

    async def fake_load_session(_path):
        raise RuntimeError("backend unreachable")

    monkeypatch.setattr(api, "load_session", fake_load_session)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.session_path = "/proj/session.jsonl"
        app.messages = prior_messages

        await app._on_session_update({})

        assert app.messages == prior_messages
