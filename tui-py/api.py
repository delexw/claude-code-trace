"""Async HTTP API client for the Claude Code Trace backend (port 11423)."""

from __future__ import annotations

import contextlib
import urllib.parse

import httpx

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

API_BASE = "http://127.0.0.1:11423"
_TIMEOUT = httpx.Timeout(30.0)


async def _get(path: str) -> object:
    async with httpx.AsyncClient(timeout=_TIMEOUT) as client:
        resp = await client.get(f"{API_BASE}{path}")
        resp.raise_for_status()
        return resp.json()


async def _post(path: str, body: object = None) -> object:
    async with httpx.AsyncClient(timeout=_TIMEOUT) as client:
        kwargs: dict = {"url": f"{API_BASE}{path}"}
        if body is not None:
            kwargs["json"] = body
        resp = await client.post(**kwargs)
        resp.raise_for_status()
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
