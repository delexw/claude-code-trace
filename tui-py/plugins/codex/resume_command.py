"""Copyable Codex commands, quoted for the shell that will receive them.

This mirrors ``src/plugins/codex/sessionCommand.ts`` so the web UI and the
terminal UI hand out the same command for the same chat. Change them together;
``tests/test_codex_resume_command.py`` repeats the cases of
``sessionCommand.test.ts`` to catch drift.
"""

from __future__ import annotations

import re

from plugins.codex.models import CodexSession

# PowerShell reads all of these as single quotes, so each has to be doubled
# inside a single-quoted string or a crafted cwd could end the string early.
_POWERSHELL_QUOTES = re.compile("['\u2018\u2019\u201a\u201b]")
_CONTROL_CHARS = re.compile(r"[\0\r\n]")
# JavaScript's `.` stops at these four line terminators; Python's stops at \n only.
_MOUNTED_DRIVE = re.compile("/mnt/([a-z])/([^\n\r\u2028\u2029]*)", re.I | re.A)
_WINDOWS_PATH = re.compile(r"[a-z]:[\\/]|\\\\|//", re.I | re.A)
_WSL_UNC_PREFIX = re.compile(r"//wsl(?:\.localhost|\$)/[^/]+(?=/)", re.I | re.A)


def _quote_posix(value: str) -> str:
    return "'" + value.replace("'", "'\\''") + "'"


def _quote_powershell(value: str) -> str:
    return "'" + _POWERSHELL_QUOTES.sub(lambda match: match.group(0) * 2, value) + "'"


def _windows_path(path: str) -> str | None:
    mounted = _MOUNTED_DRIVE.fullmatch(path)
    if mounted:
        return f"{mounted.group(1).upper()}:\\{mounted.group(2).replace('/', chr(92))}"
    return path.replace("/", "\\") if _WINDOWS_PATH.match(path) else None


def _codex_home(path: str) -> str | None:
    normalized = path.replace("\\", "/")
    marker = max(normalized.rfind("/sessions/"), normalized.rfind("/archived_sessions/"))
    return normalized[:marker] if marker > 0 else None


def resume_command(session: CodexSession, *, fork: bool = False) -> str | None:
    """The command that resumes (or forks) ``session`` in its own folder and
    Codex home, or ``None`` when no safe command can be built."""
    if not session.id or not session.cwd:
        return None
    if _CONTROL_CHARS.search(session.id + session.cwd + session.path):
        return None
    home = _codex_home(session.path)
    if home is None:
        return None
    action = "fork" if fork else "resume"
    cwd = _windows_path(session.cwd)
    if cwd is not None:
        native_home = _windows_path(home)
        if native_home is None:
            return None
        command = f"codex -C {_quote_powershell(cwd)} {action} -- {_quote_powershell(session.id)}"
        return (
            "& { $codexTracePreviousHome = $env:CODEX_HOME; try { "
            f"$env:CODEX_HOME = {_quote_powershell(native_home)}; {command} "
            "} finally { $env:CODEX_HOME = $codexTracePreviousHome } }"
        )
    # Windows auto-discovery exposes WSL homes through UNC paths. Commands are
    # pasted into that distro's POSIX shell, where the home has its native path.
    native_home = _WSL_UNC_PREFIX.sub("", home, count=1)
    if not session.cwd.startswith("/") or not native_home.startswith("/"):
        return None
    return (
        f"CODEX_HOME={_quote_posix(native_home)} codex -C {_quote_posix(session.cwd)} "
        f"{action} -- {_quote_posix(session.id)}"
    )
