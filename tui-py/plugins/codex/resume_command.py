"""Copyable Codex commands, quoted for the transcript's shell platform."""

from __future__ import annotations

import re
import shlex

from plugins.codex.models import CodexSession


def resume_command(session: CodexSession, *, fork: bool = False) -> str:
    command = "fork" if fork else "resume"
    if re.match(r"^(?:[a-z]:[\\/]|\\\\)", session.cwd, re.I):

        def quote(value: str) -> str:
            return "'" + value.replace("'", "''") + "'"

        prefix = f"Set-Location -LiteralPath {quote(session.cwd)}; " if session.cwd else ""
        return f"{prefix}codex {command} {quote(session.id)}"
    prefix = f"cd {shlex.quote(session.cwd)} && " if session.cwd else ""
    return f"{prefix}codex {command} {shlex.quote(session.id)}"
