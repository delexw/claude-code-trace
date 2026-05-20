"""TeamBoard widget — displays team snapshots and their tasks."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import Static

import theme
from data_types import TeamSnapshot
from theme import get_team_color

# Task status icons
ICON_TASK_DONE = "✓"  # U+2713
ICON_TASK_ACTIVE = "⟳"  # U+27F3
ICON_TASK_PENDING = "○"  # U+25CB
ICON_TASK_CANCELLED = "✗"  # U+2717
ICON_HRULE = "─"  # U+2500
ICON_DOT = "·"  # U+00B7


def _status_icon(status: str) -> str:
    match status.lower():
        case "completed":
            return ICON_TASK_DONE
        case "in_progress":
            return ICON_TASK_ACTIVE
        case "pending":
            return ICON_TASK_PENDING
        case "cancelled":
            return ICON_TASK_CANCELLED
        case _:
            return ICON_DOT


def _status_color(status: str) -> str:
    match status.lower():
        case "completed":
            return theme.ONGOING
        case "in_progress":
            return theme.TOKEN_HIGH
        case "pending":
            return theme.TEXT_PRIMARY
        case "cancelled":
            return theme.ERROR
        case _:
            return theme.TEXT_PRIMARY


class TeamBoard(Widget):
    """Renders team snapshots with members, tasks, and status indicators."""

    DEFAULT_CSS = """
    TeamBoard {
        height: 100%;
        overflow-y: auto;
    }
    """

    teams: reactive[list[TeamSnapshot]] = reactive(list)

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.teams = []

    def compose(self) -> ComposeResult:
        yield Static("", id="team-content")

    def on_mount(self) -> None:
        self._refresh()

    def watch_teams(self, _t) -> None:
        self._refresh()

    def _refresh(self) -> None:
        try:
            static = self.query_one("#team-content", Static)
            static.update(self._build_content())
        except Exception:
            pass

    def _build_content(self) -> str:
        active_teams = [t for t in self.teams if not t.deleted]
        if not active_teams:
            return f"[{theme.TEXT_DIM}]No active teams[/]"

        lines: list[str] = []
        rule_width = 80

        for team in active_teams:
            # Section divider: "── team-name ──────────"
            left_dash = "── "
            right_dash = " " + ICON_HRULE * max(4, rule_width - len(team.name) - len(left_dash) - 3)
            lines.append(
                f"[{theme.TEXT_MUTED}]{left_dash}[/][bold {theme.ITEM_AGENT}]{team.name}[/][{theme.TEXT_MUTED}]{right_dash}[/]"
            )

            # Summary
            done_count = sum(1 for t in team.tasks if t.status == "completed")
            summary = f"{len(team.members)} members · {done_count}/{len(team.tasks)} done"
            lines.append(f"  [{theme.TEXT_DIM}]{summary}[/]")

            # Members
            member_parts: list[str] = []
            for i, member in enumerate(team.members):
                color_name = team.member_colors.get(member, "white")
                clr = get_team_color(color_name)
                is_ongoing = team.member_ongoing.get(member, False)
                ongoing_dot = f" [{theme.ONGOING}]●[/]" if is_ongoing else ""
                sep = "  " if i > 0 else ""
                member_parts.append(f"{sep}[{clr}]{member}[/]{ongoing_dot}")
            if member_parts:
                lines.append("  " + "".join(member_parts))

            # Tasks
            for task in team.tasks:
                icon = _status_icon(task.status)
                clr = _status_color(task.status)
                owner_str = f"  [{theme.TEXT_DIM}]{task.owner}[/]" if task.owner else ""
                lines.append(
                    f"  [{theme.TEXT_DIM}]#{task.id}[/] [{clr}]{icon}[/] {task.subject}{owner_str}"
                )

            lines.append("")

        return "\n".join(lines)
