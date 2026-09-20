"""Jev efficiency-analysis types for the TUI (mirrors src/types/efficiency.ts).

These arrive from the HTTP API in camelCase — unlike the session types in
data_types.py, which are snake_case — because the Rust structs behind
/api/efficiency/* and /api/analytics/* are serialised with
`serde(rename_all = "camelCase")`.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime

import theme

# ---------------------------------------------------------------------------
# Analytics settings
# ---------------------------------------------------------------------------

PAYLOAD_MODES = ("minimized", "full-transcript")

PAYLOAD_MODE_LABELS = {
    "minimized": "Minimized data",
    "full-transcript": "Full transcript",
}

CODEX_SUBSCRIPTION = "codex-subscription"
CLAUDE_CODE_SUBSCRIPTION = "claude-code-subscription"
OPENAI_COMPATIBLE = "openai-compatible"

PROVIDER_LABELS = {
    CODEX_SUBSCRIPTION: "Codex subscription",
    CLAUDE_CODE_SUBSCRIPTION: "Claude Code subscription",
    OPENAI_COMPATIBLE: "OpenAI-compatible endpoint",
}

# Matches the web client's default for a freshly selected OpenAI-compatible
# provider; the backend rejects anything non-loopback from an HTTP client.
DEFAULT_OPENAI_BASE_URL = "http://localhost:1234/v1"


@dataclass
class JevStatus:
    configured: bool = False
    source: str | None = None
    status: str = "not_configured"


@dataclass
class RecommendationProvider:
    """One variant of the backend's internally-tagged provider enum.

    `kind` is the wire `type` tag; `base_url`/`api_key_configured` only carry
    meaning for the OpenAI-compatible variant.
    """

    kind: str = CODEX_SUBSCRIPTION
    model: str = ""
    base_url: str = ""
    api_key_configured: bool = False

    def to_dict(self) -> dict:
        if self.kind == OPENAI_COMPATIBLE:
            return {
                "type": self.kind,
                "baseUrl": self.base_url,
                "model": self.model,
                "apiKeyConfigured": self.api_key_configured,
            }
        return {"type": self.kind, "model": self.model or None}


@dataclass
class AnalyticsSettings:
    jev: JevStatus = field(default_factory=JevStatus)
    default_payload_mode: str = "minimized"
    recommendation_provider: RecommendationProvider = field(default_factory=RecommendationProvider)
    subscription_providers_available: bool = True


def jev_status_from_dict(d: dict | None) -> JevStatus:
    d = d or {}
    return JevStatus(
        configured=bool(d.get("configured", False)),
        source=d.get("source"),
        status=str(d.get("status", "not_configured")),
    )


def provider_from_dict(d: dict | None) -> RecommendationProvider:
    d = d or {}
    return RecommendationProvider(
        kind=str(d.get("type", CODEX_SUBSCRIPTION)),
        model=d.get("model") or "",
        base_url=d.get("baseUrl") or "",
        api_key_configured=bool(d.get("apiKeyConfigured", False)),
    )


def analytics_settings_from_dict(d: dict | None) -> AnalyticsSettings:
    d = d or {}
    return AnalyticsSettings(
        jev=jev_status_from_dict(d.get("jev")),
        default_payload_mode=str(d.get("defaultPayloadMode", "minimized")),
        recommendation_provider=provider_from_dict(d.get("recommendationProvider")),
        subscription_providers_available=bool(d.get("subscriptionProvidersAvailable", True)),
    )


def jev_status_label(jev: JevStatus) -> str:
    """Human-readable Jev key status, matching the web settings panel."""
    if jev.source == "environment":
        return "Configured via environment"
    return jev.status.replace("_", " ").capitalize()


# ---------------------------------------------------------------------------
# Prepared payload (what an analysis would send to Jev)
# ---------------------------------------------------------------------------


@dataclass
class EfficiencyDestination:
    provider: str = ""
    endpoint: str = ""
    model: str = ""


@dataclass
class PreparedPayload:
    session_id: str = ""
    session_path: str = ""
    payload_mode: str = "minimized"
    destination: EfficiencyDestination = field(default_factory=EfficiencyDestination)
    # The untouched response body. /api/efficiency/start takes the prepared
    # payload back verbatim, so it is posted as received rather than rebuilt
    # from the fields above.
    raw: dict = field(default_factory=dict)


def prepared_payload_from_dict(d: dict | None) -> PreparedPayload:
    d = d or {}
    dest = d.get("destination") or {}
    return PreparedPayload(
        session_id=str(d.get("sessionId", "")),
        session_path=str(d.get("sessionPath", "")),
        payload_mode=str(d.get("payloadMode", "minimized")),
        destination=EfficiencyDestination(
            provider=str(dest.get("provider", "")),
            endpoint=str(dest.get("endpoint", "")),
            model=str(dest.get("model", "")),
        ),
        raw=d,
    )


# ---------------------------------------------------------------------------
# Jobs and summaries
# ---------------------------------------------------------------------------

# Every status that is not one of these means the job is still running.
FINISHED_STATUSES = frozenset({"completed", "failed", "cancelled"})


@dataclass
class EfficiencyJob:
    analysis_id: str = ""
    session_id: str = ""
    session_path: str = ""
    session_name: str = ""
    status: str = "queued"
    progress: int | None = None
    message: str = ""
    updated_at: str = ""
    error: str | None = None
    score: int | None = None


@dataclass
class EfficiencySummary:
    session_path: str = ""
    score: int = 0
    analyzed_at: str = ""
    stale: bool = False


def _optional_int(value: object) -> int | None:
    if value is None:
        return None
    try:
        return int(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None


def job_from_dict(d: dict) -> EfficiencyJob:
    return EfficiencyJob(
        analysis_id=str(d.get("analysisId", "")),
        session_id=str(d.get("sessionId", "")),
        session_path=str(d.get("sessionPath", "")),
        session_name=str(d.get("sessionName", "")),
        status=str(d.get("status", "queued")),
        progress=_optional_int(d.get("progress")),
        message=str(d.get("message", "")),
        updated_at=str(d.get("updatedAt", "")),
        error=d.get("error"),
        score=_optional_int(d.get("score")),
    )


def summary_from_dict(d: dict) -> EfficiencySummary:
    return EfficiencySummary(
        session_path=str(d.get("sessionPath", "")),
        score=int(d.get("score", 0)),
        analyzed_at=str(d.get("analyzedAt", "")),
        stale=bool(d.get("stale", False)),
    )


def _updated_at(job: EfficiencyJob) -> datetime:
    """A job's timestamp, or the epoch when the backend sent something unparseable."""
    try:
        return datetime.fromisoformat(job.updated_at.replace("Z", "+00:00"))
    except ValueError:
        return datetime.min.replace(tzinfo=UTC)


def latest_jobs_by_session(jobs: list[EfficiencyJob]) -> dict[str, EfficiencyJob]:
    """The newest job per session — the backend keeps every attempt, and only
    the last one describes where that session stands now."""
    latest: dict[str, EfficiencyJob] = {}
    for job in jobs:
        current = latest.get(job.session_path)
        if current is None or _updated_at(job) > _updated_at(current):
            latest[job.session_path] = job
    return latest


def is_running(job: EfficiencyJob | None) -> bool:
    """True while a job is still on its way to a result."""
    return job is not None and job.status not in FINISHED_STATUSES


def status_label(status: str) -> str:
    """'processing_result' -> 'Processing result'. Unknown statuses from a
    newer backend read as themselves rather than disappearing."""
    return status.replace("_", " ").capitalize()


def score_color(score: int) -> str:
    """Score → theme colour, using the same bands as the web picker badge."""
    if score >= 87:
        return theme.ONGOING
    if score >= 65:
        return theme.ACCENT
    if score >= 40:
        return theme.TOKEN_HIGH
    return theme.ERROR
