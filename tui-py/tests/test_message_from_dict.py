"""Tests for message_from_dict, the parser for POST /api/session/message's
single-message response (the detail view's on-demand full-body fetch)."""

from __future__ import annotations

from data_types import message_from_dict


def test_parses_top_level_message_fields():
    msg = message_from_dict(
        {
            "role": "claude",
            "model": "claude-sonnet-4-20250514",
            "content": "hello",
            "items": [],
        }
    )
    assert msg.role == "claude"
    assert msg.model == "claude-sonnet-4-20250514"
    assert msg.content == "hello"


def test_parses_full_tool_body_fields_on_items():
    """The whole point of this endpoint is bodies the list view strips."""
    msg = message_from_dict(
        {
            "role": "claude",
            "items": [
                {
                    "id": "t1",
                    "item_type": "ToolCall",
                    "tool_name": "Read",
                    "tool_input": '{"file_path": "/a.py"}',
                    "tool_result": "file contents here",
                    "tool_result_json": "",
                }
            ],
        }
    )
    assert len(msg.items) == 1
    item = msg.items[0]
    assert item.tool_input == '{"file_path": "/a.py"}'
    assert item.tool_result == "file contents here"


def test_parses_nested_subagent_messages():
    msg = message_from_dict(
        {
            "role": "claude",
            "items": [
                {
                    "id": "t1",
                    "item_type": "Subagent",
                    "subagent_messages": [
                        {"role": "user", "content": "sub prompt"},
                        {"role": "claude", "content": "sub reply"},
                    ],
                }
            ],
        }
    )
    sub = msg.items[0].subagent_messages
    assert len(sub) == 2
    assert sub[0].content == "sub prompt"
    assert sub[1].content == "sub reply"
