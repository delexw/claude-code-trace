"""Antigravity's API shapes (see specs/15-session-source-plugins.md)."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class BrainFolder:
    uuid: str
    path: str
    mod_time_ms: int = 0
    title: str | None = None
    flavor: str = ""
    has_transcript: bool = False


@dataclass(frozen=True)
class ToolCall:
    name: str = ""
    summary: str = ""
    input: str = ""


@dataclass
class StepMessage:
    step_index: int = 0
    role: str = "system"
    content: str = ""
    created_at: str | None = None
    tool_calls: list[ToolCall] = field(default_factory=list)
    thinking_text: str | None = None
    error_text: str | None = None
    duration_ms: int = 0


def folder_from_dict(d: dict) -> BrainFolder:
    return BrainFolder(
        uuid=str(d.get("uuid", "")),
        path=str(d.get("path", "")),
        mod_time_ms=int(d.get("mod_time_ms") or 0),
        title=d.get("title") or None,
        flavor=str(d.get("flavor", "")),
        has_transcript=bool(d.get("has_transcript", False)),
    )


def step_from_dict(d: dict) -> StepMessage:
    return StepMessage(
        step_index=int(d.get("step_index") or 0),
        role=str(d.get("role") or "system"),
        content=str(d.get("content") or ""),
        created_at=d.get("created_at") or None,
        tool_calls=[
            ToolCall(
                name=str(c.get("name", "")),
                summary=str(c.get("summary", "")),
                input=str(c.get("input", "")),
            )
            for c in (d.get("tool_calls") or [])
            if isinstance(c, dict)
        ],
        thinking_text=d.get("thinking_text") or None,
        error_text=d.get("error_text") or None,
        duration_ms=int(d.get("duration_ms") or 0),
    )
