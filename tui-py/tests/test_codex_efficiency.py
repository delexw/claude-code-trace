"""Fabricated Codex analysis uses the shared preview/consent and progress workflow."""

from __future__ import annotations

from unittest.mock import AsyncMock, Mock

import pytest
from textual.widgets import Button, Checkbox, Static

import api
import app as app_module
from data_types import SessionInfo
from efficiency import AnalyticsSettings, EfficiencyJob, EfficiencySummary, JevStatus
from plugins.claude import ClaudePlugin
from plugins.codex.models import CodexSession
from plugins.codex.plugin import CodexPlugin
from widgets.efficiency_privacy import EfficiencyPrivacyScreen
from widgets.source_picker import SourcePicker

SESSION = CodexSession(
    id="fabricated-analysis",
    path="/example/.codex/rollout-fabricated.jsonl",
    title="Build a toy",
    cwd="/example/toy",
)
PREPARED = {
    "sessionId": SESSION.id,
    "sessionPath": SESSION.path,
    "payloadMode": "minimized",
    "destination": {"provider": "Jev", "endpoint": "https://api.typesafe.ai", "model": "jev-1"},
    "input": {"task": {"source": "codex", "firstUserMessage": "Fabricated toy request"}},
}


@pytest.mark.asyncio
async def test_codex_never_prepares_or_sends_without_a_key(monkeypatch):
    host = Mock()
    plugin = CodexPlugin(host)
    prepare, start = AsyncMock(), AsyncMock()
    monkeypatch.setattr(api, "get_analytics_settings", AsyncMock(return_value=AnalyticsSettings()))
    monkeypatch.setattr(api, "prepare_efficiency_payload", prepare)
    monkeypatch.setattr(api, "start_efficiency_analysis", start)
    await plugin.analyse_session(SESSION)
    prepare.assert_not_awaited()
    start.assert_not_awaited()
    assert "JEV_API_KEY" in host.notify.call_args.args[0]


@pytest.mark.asyncio
async def test_cached_codex_scores_and_sse_progress_update_the_existing_row(monkeypatch):
    host = Mock()
    plugin = CodexPlugin(host)
    plugin.sessions, plugin.loading = [SESSION], False
    monkeypatch.setattr(
        api,
        "list_efficiency_jobs",
        AsyncMock(
            return_value=[
                EfficiencyJob(analysis_id="example", session_path=SESSION.path, status="queued")
            ]
        ),
    )
    monkeypatch.setattr(
        api,
        "list_efficiency_summaries",
        AsyncMock(return_value=[EfficiencySummary(session_path=SESSION.path, score=82)]),
    )
    await plugin.load_efficiency_state()
    row = next(row for row in plugin.picker_section().rows if row.item == SESSION)
    assert "Jev 82" in str(row.render(0).renderables[0])
    await plugin.on_efficiency_update(
        {
            "analysisId": "example",
            "sessionPath": SESSION.path,
            "status": "analysing",
            "progress": 37,
        }
    )
    assert "37%" in str(row.render(0).renderables[0])
    host.refresh_picker_rows.assert_called_with("codex", [SESSION])


@pytest.mark.asyncio
async def test_shared_failure_stream_raises_one_notice_across_enabled_sources():
    host = Mock()
    claude, codex = ClaudePlugin(host), CodexPlugin(host)
    failure = {
        "analysisId": "example",
        "sessionPath": SESSION.path,
        "status": "failed",
        "error": "Fabricated failure",
    }
    await claude.on_efficiency_update(failure)
    await codex.on_efficiency_update(failure)
    host.notify.assert_called_once()


class _NoopSSE:
    def __init__(self, *_args, **_kwargs):
        pass

    def on(self, *_args, **_kwargs):
        pass

    def start(self):
        pass

    def stop(self):
        pass


class AnalysisApp(app_module.CCTraceApp):
    CSS_PATH = None


@pytest.mark.asyncio
async def test_analyse_key_routes_selected_source_and_reviews_every_exact_payload(monkeypatch):
    monkeypatch.setattr(app_module, "SSEClient", _NoopSSE)
    monkeypatch.setattr(
        api, "get_settings", AsyncMock(return_value={"plugins": [{"id": "codex", "enabled": True}]})
    )
    monkeypatch.setattr(api, "get_project_dirs", AsyncMock(return_value=[]))
    monkeypatch.setattr(
        api,
        "get_json",
        AsyncMock(
            return_value=[
                {"id": SESSION.id, "path": SESSION.path, "title": SESSION.title, "cwd": SESSION.cwd}
            ]
        ),
    )
    monkeypatch.setattr(api, "list_efficiency_jobs", AsyncMock(return_value=[]))
    monkeypatch.setattr(api, "list_efficiency_summaries", AsyncMock(return_value=[]))
    monkeypatch.setattr(
        api,
        "get_analytics_settings",
        AsyncMock(return_value=AnalyticsSettings(jev=JevStatus(configured=True))),
    )
    requests: list[tuple[str, dict]] = []

    async def post(endpoint, data):
        requests.append((endpoint, data))
        if endpoint == "/api/efficiency/start":
            return {"analysisId": "fabricated-job", "sessionPath": SESSION.path, "status": "queued"}
        return PREPARED

    monkeypatch.setattr(api, "_post", post)
    async with AnalysisApp().run_test() as pilot:
        app = pilot.app
        await app.workers.wait_for_complete()
        await pilot.pause()
        picker = app.query_one("#picker", SourcePicker)
        picker.index = picker.selectable_indices()[-1]
        picker.focus()
        await pilot.pause()
        assert app.check_action("plugin", ("codex", "analyse_session")) is True
        await pilot.press("a")
        await pilot.pause()
        assert isinstance(app.screen, EfficiencyPrivacyScreen)
        assert requests == [
            ("/api/codex/efficiency/prepare", {"path": SESSION.path, "payloadMode": "minimized"})
        ]
        assert "Fabricated toy request" in str(
            app.screen.query_one("#privacy-preview", Static).render()
        )
        assert any(
            "this Codex session" in str(widget.render()) for widget in app.screen.query(Static)
        )
        assert app.screen.query_one("#privacy-send", Button).disabled
        app.screen.query_one("#privacy-cancel", Button).press()
        await pilot.pause()
        assert len(requests) == 1
        await pilot.press("a")
        await pilot.pause()
        assert isinstance(app.screen, EfficiencyPrivacyScreen)
        assert app.screen.query_one("#privacy-send", Button).disabled
        app.screen.query_one("#privacy-confirm", Checkbox).value = True
        await pilot.pause()
        app.screen.query_one("#privacy-send", Button).press()
        await app.workers.wait_for_complete()
        assert requests[-1] == (
            "/api/efficiency/start",
            {"path": SESSION.path, "payload": PREPARED},
        )
        assert (
            app.plugins.get("codex").efficiency_jobs[SESSION.path].analysis_id == "fabricated-job"
        )

        # The same key still selects Claude's own prepare route when its row is highlighted.
        claude = app.plugins.get("claude")
        claude.all_sessions = [
            SessionInfo(
                path="/example/claude.jsonl",
                session_id="fabricated-claude",
                mod_time="2026-01-01T12:00:00Z",
            )
        ]
        claude.picker_loading = False
        claude.picker_error = ""
        app.sync_picker()
        await app.workers.wait_for_complete()
        await pilot.pause()
        picker.index = picker.selectable_indices()[0]
        picker.focus()
        await pilot.pause()
        assert app.highlighted_picker_item("claude") is not None
        await pilot.press("a")
        await pilot.pause()
        assert requests[-1][0] == "/api/efficiency/prepare"
        assert any(
            "this Claude Code session" in str(widget.render())
            for widget in app.screen.query(Static)
        )
        app.screen.query_one("#privacy-cancel", Button).press()
        await pilot.pause()
