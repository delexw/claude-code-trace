"""Tests for TeamBoard's task list rendering, including the empty-tasks note."""

from __future__ import annotations

from data_types import TeamSnapshot, TeamTask
from widgets.team_board import TeamBoard


def _make_widget(teams: list[TeamSnapshot]) -> TeamBoard:
    widget = TeamBoard()
    widget.teams = teams
    return widget


def test_shows_no_tasks_note_when_team_has_no_tasks():
    team = TeamSnapshot(name="alpha", members=["alice"], tasks=[])
    content = _make_widget([team])._build_content()
    assert "No tasks tracked in this session" in content


def test_renders_tasks_when_present():
    team = TeamSnapshot(
        name="alpha",
        members=["alice"],
        tasks=[TeamTask(id="1", subject="Fix bug", status="completed", owner="alice")],
    )
    content = _make_widget([team])._build_content()
    assert "Fix bug" in content
    assert "No tasks tracked in this session" not in content
