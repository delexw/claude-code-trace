"""Tests for the Jev analysis types — the camelCase wire format, the provider
payloads the backend accepts, and the pick-the-newest-job rule."""

from __future__ import annotations

import theme
from efficiency import (
    CLAUDE_CODE_SUBSCRIPTION,
    OPENAI_COMPATIBLE,
    AnalyticsSettings,
    EfficiencyJob,
    JevStatus,
    RecommendationProvider,
    analytics_settings_from_dict,
    is_running,
    jev_status_label,
    job_from_dict,
    latest_jobs_by_session,
    prepared_payload_from_dict,
    score_color,
    status_label,
    summary_from_dict,
)


def test_job_from_dict_reads_camel_case():
    job = job_from_dict(
        {
            "analysisId": "a1",
            "sessionId": "s1",
            "sessionPath": "/p/s1.jsonl",
            "sessionName": "Fix the parser",
            "status": "sending",
            "progress": 40,
            "message": "Sending to Jev",
            "updatedAt": "2026-09-20T01:02:03Z",
            "error": None,
            "score": None,
        }
    )
    assert job.analysis_id == "a1"
    assert job.session_path == "/p/s1.jsonl"
    assert job.progress == 40
    assert job.score is None


def test_job_progress_survives_a_null_or_junk_value():
    """`progress` is null between phases — that must not blow up the row."""
    assert job_from_dict({"progress": None}).progress is None
    assert job_from_dict({"progress": "soon"}).progress is None


def test_summary_from_dict_reads_camel_case():
    summary = summary_from_dict(
        {
            "sessionPath": "/p/s1.jsonl",
            "score": 82,
            "analyzedAt": "2026-09-19T00:00:00Z",
            "stale": True,
        }
    )
    assert summary.score == 82
    assert summary.stale is True


def test_analytics_settings_from_dict():
    settings = analytics_settings_from_dict(
        {
            "jev": {"configured": True, "source": "environment", "status": "configured"},
            "defaultPayloadMode": "full-transcript",
            "recommendationProvider": {
                "type": "openai-compatible",
                "baseUrl": "http://localhost:1234/v1",
                "model": "qwen",
                "apiKeyConfigured": True,
            },
            "subscriptionProvidersAvailable": False,
        }
    )
    assert settings.jev.configured is True
    assert settings.default_payload_mode == "full-transcript"
    assert settings.recommendation_provider.kind == OPENAI_COMPATIBLE
    assert settings.recommendation_provider.base_url == "http://localhost:1234/v1"
    assert settings.subscription_providers_available is False


def test_analytics_settings_defaults_when_the_backend_sends_nothing():
    settings = analytics_settings_from_dict(None)
    assert settings == AnalyticsSettings()
    assert settings.jev.configured is False


def test_subscription_provider_payload_omits_the_endpoint_fields():
    payload = RecommendationProvider(kind=CLAUDE_CODE_SUBSCRIPTION, model="").to_dict()
    assert payload == {"type": CLAUDE_CODE_SUBSCRIPTION, "model": None}


def test_openai_compatible_payload_carries_every_field_the_backend_requires():
    payload = RecommendationProvider(
        kind=OPENAI_COMPATIBLE,
        base_url="http://127.0.0.1:1234/v1",
        model="qwen",
        api_key_configured=True,
    ).to_dict()
    assert payload == {
        "type": OPENAI_COMPATIBLE,
        "baseUrl": "http://127.0.0.1:1234/v1",
        "model": "qwen",
        "apiKeyConfigured": True,
    }


def test_jev_status_label_names_an_environment_key():
    assert jev_status_label(JevStatus(configured=True, source="environment")) == (
        "Configured via environment"
    )
    assert jev_status_label(JevStatus(status="not_configured")) == "Not configured"


def test_prepared_payload_keeps_the_body_verbatim_for_the_start_call():
    body = {
        "sessionId": "s1",
        "sessionPath": "/p/s1.jsonl",
        "payloadMode": "minimized",
        "destination": {"provider": "Jev", "endpoint": "https://api.jev/x", "model": "jev-1"},
        "input": {"task": {"turns": 4}},
    }
    payload = prepared_payload_from_dict(body)
    assert payload.destination.endpoint == "https://api.jev/x"
    assert payload.raw is body


def test_latest_jobs_by_session_keeps_the_newest_attempt():
    older = EfficiencyJob(
        analysis_id="a1",
        session_path="/p/s1.jsonl",
        status="failed",
        updated_at="2026-09-19T10:00:00Z",
    )
    newer = EfficiencyJob(
        analysis_id="a2",
        session_path="/p/s1.jsonl",
        status="sending",
        updated_at="2026-09-20T10:00:00Z",
    )
    assert latest_jobs_by_session([newer, older])["/p/s1.jsonl"].analysis_id == "a2"
    assert latest_jobs_by_session([older, newer])["/p/s1.jsonl"].analysis_id == "a2"


def test_latest_jobs_by_session_tolerates_an_unparseable_timestamp():
    broken = EfficiencyJob(analysis_id="a1", session_path="/p/s1.jsonl", updated_at="whenever")
    good = EfficiencyJob(
        analysis_id="a2", session_path="/p/s1.jsonl", updated_at="2026-09-20T10:00:00Z"
    )
    assert latest_jobs_by_session([broken, good])["/p/s1.jsonl"].analysis_id == "a2"


def test_is_running_covers_every_unfinished_status():
    for status in ("queued", "preparing", "redacting", "sending", "analysing", "processing_result"):
        assert is_running(EfficiencyJob(status=status)) is True
    for status in ("completed", "failed", "cancelled"):
        assert is_running(EfficiencyJob(status=status)) is False
    assert is_running(None) is False


def test_status_label_is_readable():
    assert status_label("processing_result") == "Processing result"


def test_score_colour_bands_match_the_web_badge():
    assert score_color(95) == theme.ONGOING
    assert score_color(70) == theme.ACCENT
    assert score_color(50) == theme.TOKEN_HIGH
    assert score_color(10) == theme.ERROR
