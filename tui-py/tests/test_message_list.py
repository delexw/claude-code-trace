"""Tests for MessageList.populate — the async incremental-diff path.

The history of this code: the original sync populate raced with its own
deferred clear()/append() calls (both return AwaitComplete) so rapid
back-to-back populates duplicated rows and reset the index to None mid-
flight. These tests pin the contract: each populate completes before the
next starts, no row duplication, index stays at 0 after a clean rebuild.
"""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult

from data_types import DisplayMessage
from widgets.message_list import MessageList


class _MLApp(App):
    def compose(self) -> ComposeResult:
        yield MessageList(id="list")


def _msgs(n: int) -> list[DisplayMessage]:
    return [DisplayMessage(role="user", content=f"msg {i}") for i in range(n)]


@pytest.mark.asyncio
async def test_populate_empty_then_real_does_not_duplicate_rows():
    """Regression: when populate was sync, calling it with [] then with real
    messages produced 27 / 53 children (placeholder + appends accumulating
    before the deferred clear ran). After the async fix, n_children must
    match the new list size exactly."""
    async with _MLApp().run_test() as pilot:
        ml = pilot.app.query_one(MessageList)
        await ml.populate([], set(), False)
        await ml.populate(_msgs(5), set(), False)
        assert len(ml._nodes) == 5


@pytest.mark.asyncio
async def test_populate_sets_index_to_zero_after_full_rebuild():
    """After clear()+append cycle, ensure_highlight must place the cursor
    on row 0 so the user sees a highlighted row immediately."""
    async with _MLApp().run_test() as pilot:
        ml = pilot.app.query_one(MessageList)
        await ml.populate(_msgs(3), set(), False)
        assert ml.index == 0


@pytest.mark.asyncio
async def test_populate_incremental_preserves_user_cursor():
    """When the only change is the trailing message (streaming case), the
    user's current cursor index must be preserved — not snapped to 0."""
    async with _MLApp().run_test() as pilot:
        ml = pilot.app.query_one(MessageList)
        await ml.populate(_msgs(5), set(), False)
        ml.index = 2  # user navigated to row 2

        # New SSE update: same first 4 messages, new content on row 4
        updated = _msgs(5)
        updated[4] = DisplayMessage(role="claude", content="streamed update")
        await ml.populate(updated, set(), False)
        assert ml.index == 2, "incremental diff must not reset the user's index"
        assert len(ml._nodes) == 5


@pytest.mark.asyncio
async def test_populate_appends_new_tail_rows():
    """When a new message arrives, the incremental path should append only
    one row, not rebuild the whole list."""
    async with _MLApp().run_test() as pilot:
        ml = pilot.app.query_one(MessageList)
        await ml.populate(_msgs(3), set(), False)
        await ml.populate(_msgs(4), set(), False)
        assert len(ml._nodes) == 4


@pytest.mark.asyncio
async def test_populate_empty_shows_disabled_placeholder():
    """Empty-state branch must mount exactly one disabled ListItem."""
    async with _MLApp().run_test() as pilot:
        ml = pilot.app.query_one(MessageList)
        await ml.populate([], set(), False)
        assert len(ml._nodes) == 1
        assert ml._nodes[0].disabled is True
