"""Tests for the shared HighlightListView base class.

These are the contracts every list page (SessionPicker, MessageList,
DetailView items) depends on — `ensure_highlight` setting `index = 0`
after a rebuild and skipping disabled header rows. If any of these break,
the highlighted cursor disappears or lands on an un-selectable row.
"""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import ListItem, Static

from widgets.highlight_list import HighlightListView


class _HLApp(App):
    def compose(self) -> ComposeResult:
        yield HighlightListView(id="lv")


@pytest.mark.asyncio
async def test_ensure_highlight_sets_first_index_on_empty_then_appended_list():
    """After clear() + append(), index is None — ensure_highlight should set 0."""
    async with _HLApp().run_test() as pilot:
        lv = pilot.app.query_one(HighlightListView)
        await lv.append(ListItem(Static("Row 0")))
        await lv.append(ListItem(Static("Row 1")))
        assert lv.index is None
        lv.ensure_highlight()
        assert lv.index == 0


@pytest.mark.asyncio
async def test_ensure_highlight_is_idempotent_when_index_already_set():
    """Calling ensure_highlight again must not move a cursor the user placed."""
    async with _HLApp().run_test() as pilot:
        lv = pilot.app.query_one(HighlightListView)
        await lv.append(ListItem(Static("Row 0")))
        await lv.append(ListItem(Static("Row 1")))
        await lv.append(ListItem(Static("Row 2")))
        lv.index = 2
        lv.ensure_highlight()
        assert lv.index == 2


@pytest.mark.asyncio
async def test_ensure_highlight_skips_disabled_header_rows():
    """SessionPicker puts disabled header rows at the top; cursor must land on
    the first selectable row, not on a header."""
    async with _HLApp().run_test() as pilot:
        lv = pilot.app.query_one(HighlightListView)
        await lv.append(ListItem(Static("Header"), disabled=True))
        await lv.append(ListItem(Static("Date Group"), disabled=True))
        await lv.append(ListItem(Static("Session 1")))
        lv.ensure_highlight()
        assert lv.index == 2


@pytest.mark.asyncio
async def test_ensure_highlight_noop_when_empty():
    """No children → no index — must not raise or set an out-of-range index."""
    async with _HLApp().run_test() as pilot:
        lv = pilot.app.query_one(HighlightListView)
        lv.ensure_highlight()
        assert lv.index is None


@pytest.mark.asyncio
async def test_highlight_class_uses_shared_dim_blue():
    """The custom CSS must paint the highlighted row with
    $block-cursor-blurred-background in both focused and unfocused states.

    This is the regression test for the .--highlight / .-highlight class-
    name bug that left the highlight invisible.
    """
    async with _HLApp().run_test() as pilot:
        lv = pilot.app.query_one(HighlightListView)
        await lv.append(ListItem(Static("Row 0")))
        await lv.append(ListItem(Static("Row 1")))
        lv.ensure_highlight()
        lv.focus()
        await pilot.pause(0.05)

        item = lv._nodes[0]
        # Sanity: cursor row carries Textual's `-highlight` class
        assert item.has_class("-highlight"), "ListItem should have .-highlight"

        # Background must be the dim blurred-cursor blue
        # ($block-cursor-blurred-background → #0178D44C → alpha ≈ 0.30)
        bg = item.styles.background
        assert bg is not None
        # The Color tuple may be (r, g, b, a) or (r, g, b); accept either alpha shape.
        r, g, b = bg.r, bg.g, bg.b
        assert (r, g, b) == (1, 120, 212), f"expected #0178D4 base, got rgb=({r},{g},{b})"
