"""Builds the shell command to resume (or fork) a Claude Code session.

Mirrors `src/lib/resumeCommand.ts` — same POSIX single-quoting for `cwd`,
with an embedded `'` escaped as `'\\''`.
"""

from __future__ import annotations


def _shq(s: str) -> str:
    return "'" + s.replace("'", "'\\''") + "'"


def resume_command(cwd: str, session_id: str, fork: bool = False) -> str:
    base = f"cd {_shq(cwd)} && claude --resume {session_id}"
    return f"{base} --fork-session" if fork else base
