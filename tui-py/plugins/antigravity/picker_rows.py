"""Antigravity's rows in the shared picker: conversations, newest first."""

from __future__ import annotations

from datetime import UTC, datetime

from rich.console import Group
from rich.rule import Rule
from rich.text import Text

import theme
from format_utils import time_ago
from plugins.antigravity.models import BrainFolder
from plugins.base import PickerRow, PickerSection

ICON_SESSION = "@"
ICON_CLOCK = "~"


def conversations(folders: list[BrainFolder]) -> list[BrainFolder]:
    """The folders that hold a transcript (the rest are artifact folders), newest first."""
    return sorted(
        (f for f in folders if f.has_transcript), key=lambda f: f.mod_time_ms, reverse=True
    )


def _iso(ms: int) -> str:
    return datetime.fromtimestamp(ms / 1000, UTC).isoformat() if ms else ""


def _render(folder: BrainFolder) -> object:
    line1 = Text(folder.title or folder.uuid, style=theme.TEXT_PRIMARY)
    line2 = Text()
    if folder.flavor:
        line2.append(folder.flavor, style=theme.ACCENT)
    line2.append(f" {ICON_SESSION} {folder.uuid[:8]}", style=theme.TEXT_DIM)
    ago = time_ago(_iso(folder.mod_time_ms))
    if ago:
        line2.append(f" {ICON_CLOCK} {ago}", style=theme.TEXT_DIM)
    return Group(Text.assemble(line1, "\n", line2), Rule(style=theme.TEXT_MUTED, characters="─"))


def build_section(folders: list[BrainFolder], loading: bool, error: str) -> PickerSection:
    if loading:
        return PickerSection(loading=True)
    if error:
        return PickerSection(rows=[PickerRow(render=lambda _f: f"[{theme.ERROR}]{error}[/]")])
    shown = conversations(folders)
    if not shown:
        return PickerSection(
            rows=[
                PickerRow(
                    render=lambda _f: f"[{theme.TEXT_DIM}]No Antigravity conversations found[/]"
                )
            ]
        )
    rows = [PickerRow(render=lambda _f: f"[bold]Antigravity ({len(shown)})[/]", header=True)]
    for folder in shown:
        rows.append(PickerRow(render=lambda _f, folder=folder: _render(folder), item=folder))
    return PickerSection(rows=rows)
