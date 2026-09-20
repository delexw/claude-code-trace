"""Tests for the ProjectTree sidebar widget's rebuild behaviour."""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult

from data_types import SessionInfo
from widgets.project_tree import ProjectTree, _VimTree


def _sessions(count: int, mod_time: str = "2026-09-20T10:00:00Z") -> list[SessionInfo]:
    return [
        SessionInfo(
            session_id=f"sid-{index:08d}",
            path=f"/Users/me/.claude/projects/-Users-me-project{index:02d}/{index}.jsonl",
            cwd=f"/Users/me/project{index:02d}",
            mod_time=mod_time,
        )
        for index in range(count)
    ]


class Harness(App):
    def compose(self) -> ComposeResult:
        yield ProjectTree(id="project-tree")


async def _tree(pilot, sessions, selected=None) -> _VimTree:
    sidebar = pilot.app.query_one("#project-tree", ProjectTree)
    sidebar.update_state(sessions=sessions, selected_project=selected, collapsed_keys=set())
    await pilot.pause()
    return sidebar.query_one("#tree-inner", _VimTree)


@pytest.mark.asyncio
async def test_refresh_keeps_the_sidebar_where_the_user_scrolled_to():
    """`r` re-discovers every session and rebuilds every node — that must not
    throw the user back to the top of a long project list."""
    async with Harness().run_test(size=(40, 12)) as pilot:
        tree = await _tree(pilot, _sessions(30))
        tree.scroll_to(y=12, animate=False)
        await pilot.pause()
        assert tree.scroll_offset.y == 12

        # A refresh: same projects, newer timestamps.
        tree = await _tree(pilot, _sessions(30, mod_time="2026-09-20T11:00:00Z"))

        assert tree.scroll_offset.y == 12


@pytest.mark.asyncio
async def test_scrolling_past_a_shrunken_tree_lands_at_its_end():
    async with Harness().run_test(size=(40, 12)) as pilot:
        tree = await _tree(pilot, _sessions(30))
        tree.scroll_to(y=20, animate=False)
        await pilot.pause()
        scrolled_to = tree.scroll_offset.y
        assert scrolled_to > 0

        tree = await _tree(pilot, _sessions(3))

        assert tree.scroll_offset.y < scrolled_to
        assert tree.scroll_offset.y == tree.max_scroll_y


@pytest.mark.asyncio
async def test_a_rebuild_still_marks_the_selected_project():
    """Restoring the scroll must not cost the selected-project marker."""
    async with Harness().run_test(size=(40, 12)) as pilot:
        await _tree(pilot, _sessions(5), selected="-Users-me-project03")

        tree = await _tree(
            pilot, _sessions(5, mod_time="2026-09-20T11:00:00Z"), selected="-Users-me-project03"
        )

        labels = [str(node.label) for node in tree.root.children]
        assert any("▸ project03" in label for label in labels)
