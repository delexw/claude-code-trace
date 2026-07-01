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
from widgets.detail_view import DetailView, _render_edit_diff, _render_msg_title


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


@pytest.mark.asyncio
async def test_tool_call_input_label_has_no_border_but_json_box_does():
    """ "Input"/"Result" are plain headings above the box, not boxes themselves.

    Regression test: an earlier refactor moved the label into its own
    Markdown widget, which `_ItemListView`'s CSS always borders — producing
    a separate bordered box around just the word "Input" before the actual
    JSON content box.
    """
    async with _DVApp().run_test() as pilot:
        dv = pilot.app.query_one(DetailView)
        item = DisplayItem(
            id="t1",
            item_type="ToolCall",
            tool_name="Bash",
            tool_input='{"command": "ls"}',
        )
        msg = DisplayMessage(role="claude", content="x", items=[item])
        dv.populate(message=msg, expanded_items={0}, ongoing=False, depth=0)
        await pilot.pause(0.2)

        label = dv.query_one("#item-0 Static.item-label", Static)
        assert label.styles.border.top[0] == "", "label must not be boxed"

        box = dv.query_one("#item-0 Static.diff-block", Static)
        assert box.styles.border.top[0] == "round", "JSON content must stay boxed"


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


# ---------------------------------------------------------------------------
# _render_edit_diff
# ---------------------------------------------------------------------------


def _spans_with(text, needle):
    """Return (covered_text, style) for spans whose style contains `needle`."""
    return [
        (text.plain[s.start : s.end], str(s.style)) for s in text.spans if needle in str(s.style)
    ]


def test_render_edit_diff_colors_lines_and_changed_words():
    import json

    inp = json.dumps(
        {
            "file_path": "/src/main.ts",
            "old_string": "const x = 1;",
            "new_string": "const x = 2;\nconst y = 3;",
        }
    )
    text = _render_edit_diff(inp)
    assert text is not None
    # No guillemets — colour conveys the change now.
    assert "«" not in text.plain
    # File path header is present and styled.
    assert "/src/main.ts" in text.plain
    # Unified diff markers and lines are in the plain text.
    assert "-const x = 1;" in text.plain
    assert "+const x = 2;" in text.plain
    assert "+const y = 3;" in text.plain
    # The changed token "1" carries the removed-word background tint.
    removed_words = _spans_with(text, "#67060c")
    assert ("1", "bold #f85149 on #67060c") in removed_words
    # The changed token "2" carries the added-word background tint.
    added_words = _spans_with(text, "#033a16")
    assert ("2", "bold #3fb950 on #033a16") in added_words
    # The unpaired inserted line has no changed-word spans (whole line, not a token).
    assert all(covered != "const y = 3;" for covered, _ in added_words)


def test_render_edit_diff_keeps_context_lines():
    import json

    inp = json.dumps(
        {
            "file_path": "a.ts",
            "old_string": "keep\nfoo bar\ntail",
            "new_string": "keep\nfoo baz\ntail",
        }
    )
    text = _render_edit_diff(inp)
    assert text is not None
    # Unchanged lines are context (space-prefixed), not removed+added.
    assert " keep" in text.plain
    assert " tail" in text.plain
    assert "-foo bar" in text.plain
    assert "+foo baz" in text.plain
    # Only the changed words are highlighted.
    assert ("bar", "bold #f85149 on #67060c") in _spans_with(text, "#67060c")
    assert ("baz", "bold #3fb950 on #033a16") in _spans_with(text, "#033a16")


def test_render_edit_diff_shows_replace_all():
    import json

    inp = json.dumps(
        {"file_path": "a.ts", "old_string": "foo", "new_string": "bar", "replace_all": True}
    )
    text = _render_edit_diff(inp)
    assert text is not None
    assert "(replace all)" in text.plain


def test_render_edit_diff_returns_none_for_non_edit():
    import json

    assert _render_edit_diff(json.dumps({"path": "file.ts"})) is None
    assert _render_edit_diff("not json") is None
