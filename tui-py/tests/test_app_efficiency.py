"""Tests for the app's Jev wiring: the scan bar, analysis progress arriving
over SSE, and the confirm-before-sending flow behind the `a` key."""

from __future__ import annotations

import pytest
from textual.widgets import Button, Checkbox, Input

import api
import app as app_module
from data_types import SessionInfo
from efficiency import (
    AnalyticsSettings,
    EfficiencyJob,
    EfficiencySummary,
    JevStatus,
    RecommendationProvider,
    prepared_payload_from_dict,
)
from widgets.analytics_settings import AnalyticsSettingsScreen
from widgets.efficiency_privacy import EfficiencyPrivacyScreen
from widgets.index_progress import IndexProgressBar

SESSION = SessionInfo(
    session_id="sid-analyse01", path="/p/s1.jsonl", mod_time="2026-09-20T10:00:00Z"
)

PREPARED = {
    "sessionId": "sid-analyse01",
    "sessionPath": "/p/s1.jsonl",
    "payloadMode": "minimized",
    "destination": {"provider": "Jev", "endpoint": "https://api.typesafe.ai", "model": "jev-1"},
    "input": {"task": {"turns": 3}},
}

CONFIGURED = AnalyticsSettings(jev=JevStatus(configured=True, source="environment"))
CONFIGURED_SETTINGS = AnalyticsSettings(
    jev=JevStatus(configured=True, source="environment"),
    recommendation_provider=RecommendationProvider(model="gpt-5-codex"),
)
UNCONFIGURED = AnalyticsSettings(jev=JevStatus(configured=False, status="not_configured"))


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


@pytest.fixture(autouse=True)
def _stub_network(monkeypatch):
    """Keep App.on_mount's background workers off a real backend."""
    monkeypatch.setattr(app_module, "SSEClient", _NoopSSE)

    async def no_dirs():
        return []

    async def no_jobs():
        return []

    async def no_summaries():
        return []

    monkeypatch.setattr(api, "get_project_dirs", no_dirs)
    monkeypatch.setattr(api, "list_efficiency_jobs", no_jobs)
    monkeypatch.setattr(api, "list_efficiency_summaries", no_summaries)


def _capture_notices(app) -> list[tuple[str, str]]:
    notices: list[tuple[str, str]] = []
    app.notify = lambda message, **kwargs: notices.append(
        (message, kwargs.get("severity", "information"))
    )
    return notices


# ---------------------------------------------------------------------------
# Scan progress
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_index_progress_event_drives_the_scan_bar():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app._on_index_progress(
            {"files_read": 5, "total_files": 10, "bytes_read": 250, "total_bytes": 1000}
        )
        await pilot.pause()

        assert app.index_progress is not None
        assert "25%" in app.query_one("#index-progress", IndexProgressBar).content.plain


# ---------------------------------------------------------------------------
# Analysis progress
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_efficiency_update_is_kept_against_its_session():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await app._on_efficiency_update(
            {"analysisId": "a1", "sessionPath": "/p/s1.jsonl", "status": "sending", "progress": 30}
        )
        assert app.efficiency_jobs["/p/s1.jsonl"].progress == 30


@pytest.mark.asyncio
async def test_a_completed_analysis_pulls_in_the_new_score(monkeypatch):
    """The score lands in the summaries, not in the finished job."""

    async def fake_summaries():
        return [EfficiencySummary(session_path="/p/s1.jsonl", score=88)]

    monkeypatch.setattr(api, "list_efficiency_summaries", fake_summaries)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await app._on_efficiency_update(
            {"analysisId": "a1", "sessionPath": "/p/s1.jsonl", "status": "completed"}
        )
        assert app.efficiency_summaries["/p/s1.jsonl"].score == 88


@pytest.mark.asyncio
async def test_an_update_without_a_session_path_is_ignored():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await app._on_efficiency_update({"analysisId": "a1"})
        assert app.efficiency_jobs == {}


@pytest.mark.asyncio
async def test_startup_picks_up_analyses_run_from_other_clients(monkeypatch):
    async def fake_jobs():
        return [
            EfficiencyJob(
                analysis_id="old",
                session_path="/p/s1.jsonl",
                status="failed",
                updated_at="2026-09-19T10:00:00Z",
            ),
            EfficiencyJob(
                analysis_id="new",
                session_path="/p/s1.jsonl",
                status="analysing",
                updated_at="2026-09-20T10:00:00Z",
            ),
        ]

    async def fake_summaries():
        return [EfficiencySummary(session_path="/p/s2.jsonl", score=64)]

    monkeypatch.setattr(api, "list_efficiency_jobs", fake_jobs)
    monkeypatch.setattr(api, "list_efficiency_summaries", fake_summaries)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await app._load_efficiency_state()

        assert app.efficiency_jobs["/p/s1.jsonl"].analysis_id == "new"
        assert app.efficiency_summaries["/p/s2.jsonl"].score == 64


# ---------------------------------------------------------------------------
# Starting an analysis
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_no_data_is_prepared_without_a_jev_key(monkeypatch):
    async def fake_settings():
        return UNCONFIGURED

    async def fail_prepare(*_args, **_kwargs):
        raise AssertionError("nothing may be prepared before a key is configured")

    monkeypatch.setattr(api, "get_analytics_settings", fake_settings)
    monkeypatch.setattr(api, "prepare_efficiency_payload", fail_prepare)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        notices = _capture_notices(app)
        await app._analyse_session(SESSION)

        assert notices and notices[0][1] == "warning"
        assert "JEV_API_KEY" in notices[0][0]


@pytest.mark.asyncio
async def test_nothing_is_sent_until_the_privacy_notice_is_accepted(monkeypatch):
    prepared_for: list[tuple[str, str]] = []

    async def fake_settings():
        return CONFIGURED

    async def fake_prepare(path, payload_mode):
        prepared_for.append((path, payload_mode))
        return prepared_payload_from_dict(PREPARED)

    async def fail_start(_payload):
        raise AssertionError("a cancelled notice must not send anything")

    monkeypatch.setattr(api, "get_analytics_settings", fake_settings)
    monkeypatch.setattr(api, "prepare_efficiency_payload", fake_prepare)
    monkeypatch.setattr(api, "start_efficiency_analysis", fail_start)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        worker = app.run_worker(app._analyse_session(SESSION))
        await pilot.pause()

        assert prepared_for == [("/p/s1.jsonl", "minimized")]
        assert isinstance(app.screen, EfficiencyPrivacyScreen)

        app.screen.query_one("#privacy-cancel", Button).press()
        await worker.wait()
        assert app.efficiency_jobs == {}


@pytest.mark.asyncio
async def test_confirming_sends_the_prepared_payload_and_tracks_the_job(monkeypatch):
    sent: list[dict] = []

    async def fake_settings():
        return CONFIGURED

    async def fake_prepare(_path, _payload_mode):
        return prepared_payload_from_dict(PREPARED)

    async def fake_start(payload):
        sent.append(payload.raw)
        return EfficiencyJob(
            analysis_id="a1", session_path="/p/s1.jsonl", status="queued", progress=0
        )

    monkeypatch.setattr(api, "get_analytics_settings", fake_settings)
    monkeypatch.setattr(api, "prepare_efficiency_payload", fake_prepare)
    monkeypatch.setattr(api, "start_efficiency_analysis", fake_start)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        worker = app.run_worker(app._analyse_session(SESSION))
        await pilot.pause()

        app.screen.query_one("#privacy-confirm", Checkbox).value = True
        await pilot.pause()
        app.screen.query_one("#privacy-send", Button).press()
        await worker.wait()

        # Posted back exactly as prepared — the backend re-reads every field.
        assert sent == [PREPARED]
        assert app.efficiency_jobs["/p/s1.jsonl"].analysis_id == "a1"


@pytest.mark.asyncio
async def test_a_backend_refusal_is_reported_verbatim(monkeypatch):
    async def fake_settings():
        return CONFIGURED

    async def failing_prepare(_path, _payload_mode):
        raise RuntimeError("Session transcript is empty")

    monkeypatch.setattr(api, "get_analytics_settings", fake_settings)
    monkeypatch.setattr(api, "prepare_efficiency_payload", failing_prepare)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        notices = _capture_notices(app)
        await app._analyse_session(SESSION)

        assert notices == [("Session transcript is empty", "error")]


# ---------------------------------------------------------------------------
# Where the keys apply
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_the_jev_keys_are_offered_in_the_picker_only():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        assert app.check_action("analyse_session", ()) is True
        assert app.check_action("analytics_settings", ()) is True

        app.view = "list"
        assert app.check_action("analyse_session", ()) is False
        assert app.check_action("analytics_settings", ()) is False


@pytest.mark.asyncio
async def test_an_open_modal_keeps_the_app_s_own_keys_out_of_its_way():
    """Without this the priority bindings would steal Escape from the modal
    and swallow characters typed into its inputs."""
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.push_screen(EfficiencyPrivacyScreen(prepared_payload_from_dict(PREPARED)))
        await pilot.pause()

        assert app.check_action("back_or_quit", ()) is False
        assert app.check_action("refresh", ()) is False
        assert app.check_action("focused_cursor_down", ()) is False
        # Only this app's own actions. Textual's live on the app too, and a
        # modal's Tab binding runs them — disabling those left the modal
        # navigable by mouse only.
        assert app.check_action("focus_next", ()) is True
        assert app.check_action("focus_previous", ()) is True


@pytest.mark.asyncio
async def test_tab_walks_a_modal_s_own_controls():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.push_screen(AnalyticsSettingsScreen(CONFIGURED_SETTINGS))
        await pilot.pause()

        focused: list[str | None] = []
        for _ in range(4):
            await pilot.press("tab")
            await pilot.pause()
            focused.append(app.focused.id if app.focused else None)

        assert focused == ["test-jev", "payload-mode", "provider-kind", "provider-model"]


@pytest.mark.asyncio
async def test_a_radio_set_toggles_from_the_keyboard():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.push_screen(AnalyticsSettingsScreen(CONFIGURED_SETTINGS))
        await pilot.pause()

        app.screen.query_one("#payload-mode").focus()
        await pilot.pause()
        await pilot.press("down")
        await pilot.press("enter")
        await pilot.pause()

        assert app.screen.query_one("#payload-mode").pressed_button.id == "mode-full-transcript"


@pytest.mark.asyncio
async def test_letters_the_app_binds_are_typed_into_a_modal_input():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        app.push_screen(AnalyticsSettingsScreen(CONFIGURED_SETTINGS))
        await pilot.pause()

        model = app.screen.query_one("#provider-model", Input)
        model.value = ""
        model.focus()
        await pilot.pause()
        for character in "qjcegraudyhlk":
            await pilot.press(character)
        await pilot.pause()

        assert model.value == "qjcegraudyhlk"
        assert isinstance(app.screen, AnalyticsSettingsScreen)


@pytest.mark.asyncio
async def test_pressing_a_analyses_the_highlighted_session(monkeypatch):
    prepared_for: list[str] = []

    async def fake_dirs():
        return ["/p"]

    async def fake_discover(_dirs):
        return [SESSION]

    async def fake_watch(_dirs):
        return None

    async def fake_settings():
        return CONFIGURED

    async def fake_prepare(path, _payload_mode):
        prepared_for.append(path)
        return prepared_payload_from_dict(PREPARED)

    monkeypatch.setattr(api, "get_project_dirs", fake_dirs)
    monkeypatch.setattr(api, "discover_sessions", fake_discover)
    monkeypatch.setattr(api, "watch_picker", fake_watch)
    monkeypatch.setattr(api, "get_analytics_settings", fake_settings)
    monkeypatch.setattr(api, "prepare_efficiency_payload", fake_prepare)

    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        await pilot.pause()
        app.query_one("#picker").focus()
        await pilot.pause()

        await pilot.press("a")
        await pilot.pause()

        assert prepared_for == ["/p/s1.jsonl"]
        assert isinstance(app.screen, EfficiencyPrivacyScreen)


@pytest.mark.asyncio
async def test_a_failed_analysis_is_raised_not_only_written_into_the_row():
    """An analysis takes minutes — by the time it fails the user is usually
    looking at something else."""
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        notices = _capture_notices(app)

        await app._on_efficiency_update(
            {
                "analysisId": "a1",
                "sessionPath": "/p/s1.jsonl",
                "sessionName": "Fix login",
                "status": "failed",
                "message": "Efficiency analysis failed",
                "error": "Jev did not respond within 60s",
            }
        )

        assert notices == [("Fix login: Jev did not respond within 60s", "error")]


@pytest.mark.asyncio
async def test_a_failure_without_detail_falls_back_to_the_job_message():
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        notices = _capture_notices(app)

        await app._on_efficiency_update(
            {
                "analysisId": "a1",
                "sessionPath": "/p/s1.jsonl",
                "sessionId": "sid-one",
                "status": "failed",
                "message": "Could not save analysis",
                "error": None,
            }
        )

        assert notices == [("sid-one: Could not save analysis", "error")]


@pytest.mark.asyncio
async def test_a_cancelled_analysis_says_nothing():
    """The user cancelled it; telling them it stopped is noise."""
    async with app_module.CCTraceApp().run_test() as pilot:
        app = pilot.app
        notices = _capture_notices(app)

        await app._on_efficiency_update(
            {
                "analysisId": "a1",
                "sessionPath": "/p/s1.jsonl",
                "status": "cancelled",
                "message": "Analysis cancelled",
            }
        )

        assert notices == []
