"""Tests for the DetailView pane.

Covers the visual contract added on top of the populate flow:
- Outer bordered container.
- RESPONSE / STEP section headings.
- Heading visibility tied to message + item presence.
"""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Collapsible, Static

from data_types import DisplayItem, DisplayMessage
from items import get_item_summary
from widgets.detail_view import DetailView, _render_msg_title


class _DVApp(App):
    def compose(self) -> ComposeResult:
        yield DetailView(id="detail")


@pytest.mark.asyncio
async def test_detail_view_has_visible_border():
    """The DetailView pane must render with a rounded border so the user
    can see the boundary between header and items list."""
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        edges = dv.styles.border
        # border: round on all four edges
        assert edges.top[0] == "round"
        assert edges.bottom[0] == "round"
        assert edges.left[0] == "round"
        assert edges.right[0] == "round"


@pytest.mark.asyncio
async def test_response_heading_shown_when_message_present():
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        msg = DisplayMessage(role="claude", content="hello", items=[])
        dv.populate(message=msg, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        heading = dv.query_one("#msg-heading", Static)
        assert heading.display is True
        assert "RESPONSE" in str(heading.render())


@pytest.mark.asyncio
async def test_response_heading_hidden_when_no_message():
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        dv.populate(message=None, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        heading = dv.query_one("#msg-heading", Static)
        assert heading.display is False


@pytest.mark.asyncio
async def test_step_heading_shows_item_count():
    """Step heading must include the live item count `── STEP (N) ──`."""
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        items = [
            DisplayItem(id=f"i{i}", item_type="ToolCall", tool_name=f"T{i}") for i in range(12)
        ]
        msg = DisplayMessage(role="claude", content="x", items=items)
        dv.populate(message=msg, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        heading = dv.query_one("#items-heading", Static)
        assert heading.display is True
        text = str(heading.render())
        assert "STEP" in text
        assert "(12)" in text


@pytest.mark.asyncio
async def test_step_heading_hidden_when_no_items():
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        msg = DisplayMessage(role="user", content="just a message", items=[])
        dv.populate(message=msg, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        heading = dv.query_one("#items-heading", Static)
        assert heading.display is False


def test_output_summary_is_empty_so_prose_is_not_duplicated():
    item = DisplayItem(id="o", item_type="Output", text="a" * 100)
    assert get_item_summary(item) == ""


def test_msg_title_omits_content_preview_when_output_items_present():
    msg = DisplayMessage(
        role="claude",
        content="I'll investigate the tmp agents",
        items=[DisplayItem(id="o", item_type="Output", text="I'll investigate the tmp agents")],
    )
    title = _render_msg_title(msg, depth=0, ongoing=False)
    assert "investigate" not in title


def test_msg_title_keeps_content_preview_without_output_items():
    msg = DisplayMessage(
        role="claude",
        content="plain answer",
        items=[DisplayItem(id="t", item_type="ToolCall", tool_name="Read")],
    )
    title = _render_msg_title(msg, depth=0, ongoing=False)
    assert "plain answer" in title


@pytest.mark.asyncio
async def test_content_blob_collapsed_when_output_items_present():
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        msg = DisplayMessage(
            role="claude",
            content="prose here",
            items=[DisplayItem(id="o", item_type="Output", text="prose here")],
        )
        dv.populate(message=msg, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        coll = dv.query_one("#msg-content", Collapsible)
        assert coll.collapsed is True


@pytest.mark.asyncio
async def test_content_blob_shown_without_output_items():
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        msg = DisplayMessage(
            role="claude",
            content="plain answer",
            items=[DisplayItem(id="t", item_type="ToolCall", tool_name="Read")],
        )
        dv.populate(message=msg, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        coll = dv.query_one("#msg-content", Collapsible)
        assert coll.collapsed is False


@pytest.mark.asyncio
async def test_output_item_expanded_even_when_not_in_expanded_set():
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        msg = DisplayMessage(
            role="claude",
            content="",
            items=[DisplayItem(id="o", item_type="Output", text="prose here")],
        )
        dv.populate(message=msg, expanded_items=set(), ongoing=False, depth=0)
        await pilot.pause(0.2)
        coll = dv.query_one("#item-0", Collapsible)
        assert coll.collapsed is False
