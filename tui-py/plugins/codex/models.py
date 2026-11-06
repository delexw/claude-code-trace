"""Codex picker metadata supplied by the backend."""

from dataclasses import dataclass


@dataclass(frozen=True)
class CodexSession:
    id: str = ""
    path: str = ""
    title: str = ""
    cwd: str = ""
    model: str = ""
    modified_at: str = ""
    archived: bool = False
    source: str = ""


def session_from_dict(data: dict) -> CodexSession:
    return CodexSession(
        **{key: data[key] for key in CodexSession.__dataclass_fields__ if key in data}
    )
