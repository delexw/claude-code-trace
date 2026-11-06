"""Codex picker metadata supplied by the backend."""

from dataclasses import dataclass, field


@dataclass(frozen=True)
class CodexSession:
    id: str = ""
    path: str = ""
    title: str = ""
    cwd: str = ""
    dirs: list[str] = field(default_factory=list)
    forked_from_session_id: str | None = None
    model: str = ""
    git_branch: str = ""
    permission_mode: str = ""
    sandbox_policy: str = ""
    started_at: str = ""
    last_activity_at: str = ""
    duration_ms: int | None = None
    turn_count: int = 0
    is_ongoing: bool = False
    context_tokens: int | None = None
    context_window: int | None = None
    modified_at: str = ""
    archived: bool = False
    source: str = ""
    total_tokens: int = 0
    cost_usd: float | None = None
    cost_partial: bool = False


def session_from_dict(data: dict) -> CodexSession:
    return CodexSession(
        **{key: data[key] for key in CodexSession.__dataclass_fields__ if key in data}
    )
