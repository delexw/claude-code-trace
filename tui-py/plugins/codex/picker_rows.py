"""Codex conversations in the shared terminal picker."""

from rich.console import Group
from rich.rule import Rule
from rich.text import Text

import theme
from format_utils import time_ago
from plugins.base import PickerRow, PickerSection
from plugins.codex.models import CodexSession


def _render(session: CodexSession) -> Group:
    title = Text(session.title or session.id, style=theme.TEXT_PRIMARY)
    metadata = Text(session.source, style=theme.ACCENT)
    if session.archived:
        metadata.append("  Archived", style=theme.TEXT_DIM)
    if session.cwd:
        metadata.append(f"  {session.cwd}", style=theme.TEXT_DIM)
    if ago := time_ago(session.modified_at):
        metadata.append(f"  {ago}", style=theme.TEXT_DIM)
    return Group(Text.assemble(title, "\n", metadata), Rule(style=theme.TEXT_MUTED))


def build_section(sessions: list[CodexSession], loading: bool, error: str) -> PickerSection:
    if loading:
        return PickerSection(loading=True)
    if error:
        return PickerSection(rows=[PickerRow(render=lambda _f: Text(error, style=theme.ERROR))])
    if not sessions:
        return PickerSection(
            rows=[
                PickerRow(
                    render=lambda _f: Text("No Codex conversations found", style=theme.TEXT_DIM)
                )
            ]
        )
    rows = [
        PickerRow(render=lambda _f: Text(f"Codex ({len(sessions)})", style="bold"), header=True)
    ]
    rows.extend(
        PickerRow(render=lambda _f, session=session: _render(session), item=session)
        for session in sorted(sessions, key=lambda session: session.modified_at, reverse=True)
    )
    return PickerSection(rows=rows)
