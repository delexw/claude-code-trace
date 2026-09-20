"""Async HTTP API client for the Claude Code Trace backend (port 11423)."""

from __future__ import annotations

import contextlib
import urllib.parse

import httpx

import auth
from data_types import (
    DebugEntry,
    DisplayMessage,
    LoadResult,
    SessionInfo,
    debug_entry_from_dict,
    load_result_from_dict,
    message_from_dict,
    session_info_from_dict,
)
from efficiency import (
    AnalyticsSettings,
    EfficiencyJob,
    EfficiencySummary,
    PreparedPayload,
    RecommendationProvider,
    analytics_settings_from_dict,
    job_from_dict,
    prepared_payload_from_dict,
    summary_from_dict,
)

API_BASE = "http://127.0.0.1:11423"
_TIMEOUT = httpx.Timeout(30.0)


class ApiAuthError(httpx.HTTPStatusError):
    """The backend rejected the call because this TUI did not present a valid
    ``tui`` client credential (HTTP 401). See ``auth.py`` for where it comes from."""


class ApiRequestError(httpx.HTTPStatusError):
    """A non-401 failure the backend explained in its response body — e.g. a
    missing Jev API key, or a provider URL web clients may not use. Carries
    that explanation as its message so it can be shown to the user as-is."""


def _error_detail(resp: httpx.Response) -> str:
    """The backend's ``{"error": ...}`` text, or "" when the body has none.

    The body may be empty or non-JSON (a proxy's error page): never let the
    error explaining a failure raise its own error.
    """
    try:
        body = resp.json()
    except Exception:  # noqa: BLE001 — any parse failure just means "no detail"
        return ""
    detail = body.get("error") if isinstance(body, dict) else None
    return str(detail) if detail else ""


def _auth_error_message(resp: httpx.Response) -> str:
    path = auth.credential_path()
    if auth.resolve_credential() is None:
        why = (
            f"The TUI sent no client credential: {path} is missing or empty (the backend "
            f"writes it on start), or {auth.ENV_AUTH}=off is set for the TUI but not the backend."
        )
    else:
        why = (
            f"Backend rejected the TUI's client credential from {path}. Run the TUI as the "
            "same user as the backend, or reissue the `tui` client in "
            "Settings > Accepted clients."
        )
    detail = _error_detail(resp)
    return f"{why} Backend said: {detail}" if detail else why


def _raise_for_status(resp: httpx.Response) -> None:
    if resp.status_code == 401:
        raise ApiAuthError(_auth_error_message(resp), request=resp.request, response=resp)
    if resp.status_code >= 400:
        detail = _error_detail(resp)
        if detail:
            raise ApiRequestError(detail, request=resp.request, response=resp)
    resp.raise_for_status()


async def _get(path: str) -> object:
    async with httpx.AsyncClient(timeout=_TIMEOUT) as client:
        resp = await client.get(f"{API_BASE}{path}", headers=auth.auth_headers())
        _raise_for_status(resp)
        return resp.json()


async def _post(path: str, body: object = None) -> object:
    async with httpx.AsyncClient(timeout=_TIMEOUT) as client:
        kwargs: dict = {"url": f"{API_BASE}{path}", "headers": auth.auth_headers()}
        if body is not None:
            kwargs["json"] = body
        resp = await client.post(**kwargs)
        _raise_for_status(resp)
        text = resp.text.strip()
        if text:
            return resp.json()
        return None


async def get_project_dirs() -> list[str]:
    data = await _get("/api/project-dirs")
    return list(data)  # type: ignore[arg-type]


async def discover_sessions(dirs: list[str]) -> list[SessionInfo]:
    data = await _post("/api/sessions", {"dirs": dirs})
    return [session_info_from_dict(d) for d in (data or [])]  # type: ignore[union-attr]


async def load_session(path: str) -> LoadResult:
    data = await _post("/api/session/load", {"path": path})
    return load_result_from_dict(data)  # type: ignore[arg-type]


async def load_message(path: str, index: int) -> DisplayMessage | None:
    """Fetch the full (heavy-body) message at `index` for the detail view.

    `load_session`'s messages have tool_input/tool_result/tool_result_json
    stripped to keep the list view light — this fetches one full message
    on demand instead.
    """
    data = await _post("/api/session/message", {"path": path, "index": index})
    return message_from_dict(data) if data else None  # type: ignore[arg-type]


async def watch_session(path: str) -> None:
    await _post("/api/session/watch", {"path": path})


async def unwatch_session() -> None:
    with contextlib.suppress(Exception):
        await _post("/api/session/unwatch")


async def get_debug_log(session_path: str) -> list[DebugEntry]:
    encoded = urllib.parse.quote(session_path, safe="")
    data = await _get(f"/api/debug-log?path={encoded}")
    return [debug_entry_from_dict(d) for d in (data or [])]  # type: ignore[union-attr]


async def watch_picker(project_dirs: list[str]) -> None:
    await _post("/api/picker/watch", {"projectDirs": project_dirs})


async def unwatch_picker() -> None:
    with contextlib.suppress(Exception):
        await _post("/api/picker/unwatch")


# ---------------------------------------------------------------------------
# Analytics settings and Jev efficiency analysis
# ---------------------------------------------------------------------------


async def get_analytics_settings() -> AnalyticsSettings:
    data = await _get("/api/analytics/settings")
    return analytics_settings_from_dict(data)  # type: ignore[arg-type]


async def set_analytics_settings(
    payload_mode: str, provider: RecommendationProvider
) -> AnalyticsSettings:
    data = await _post(
        "/api/analytics/settings",
        {"defaultPayloadMode": payload_mode, "recommendationProvider": provider.to_dict()},
    )
    return analytics_settings_from_dict(data)  # type: ignore[arg-type]


async def test_jev_connection() -> None:
    """Raises if the key is missing or Jev rejects it; returns None on success."""
    await _post("/api/analytics/jev/test")


async def test_recommendation_provider(provider: RecommendationProvider) -> None:
    await _post("/api/analytics/recommendation/test", {"provider": provider.to_dict()})


async def prepare_efficiency_payload(path: str, payload_mode: str) -> PreparedPayload:
    """Build (and locally redact) the payload an analysis would send to Jev.

    Nothing leaves the machine until `start_efficiency_analysis` is called
    with it.
    """
    data = await _post("/api/efficiency/prepare", {"path": path, "payloadMode": payload_mode})
    return prepared_payload_from_dict(data)  # type: ignore[arg-type]


async def start_efficiency_analysis(payload: PreparedPayload) -> EfficiencyJob:
    """Send a prepared payload to Jev. The backend reports progress from here
    on as `efficiency-analysis-update` SSE events."""
    data = await _post(
        "/api/efficiency/start", {"path": payload.session_path, "payload": payload.raw}
    )
    return job_from_dict(data)  # type: ignore[arg-type]


async def list_efficiency_jobs() -> list[EfficiencyJob]:
    data = await _get("/api/efficiency/jobs")
    return [job_from_dict(d) for d in (data or [])]  # type: ignore[union-attr]


async def list_efficiency_summaries() -> list[EfficiencySummary]:
    data = await _get("/api/efficiency/summaries")
    return [summary_from_dict(d) for d in (data or [])]  # type: ignore[union-attr]
