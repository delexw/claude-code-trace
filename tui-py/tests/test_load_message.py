"""Tests for api.load_message — the detail view's on-demand full-body fetch."""

from __future__ import annotations

import api


async def test_load_message_posts_path_and_index(monkeypatch):
    seen = {}

    async def fake_post(path, body=None):
        seen["path"] = path
        seen["body"] = body
        return {"role": "claude", "content": "hi", "items": []}

    monkeypatch.setattr(api, "_post", fake_post)

    msg = await api.load_message("/proj/s.jsonl", 42)

    assert seen["path"] == "/api/session/message"
    assert seen["body"] == {"path": "/proj/s.jsonl", "index": 42}
    assert msg is not None
    assert msg.content == "hi"


async def test_load_message_returns_none_when_index_out_of_range(monkeypatch):
    async def fake_post(path, body=None):
        return None

    monkeypatch.setattr(api, "_post", fake_post)

    msg = await api.load_message("/proj/s.jsonl", 999)

    assert msg is None
