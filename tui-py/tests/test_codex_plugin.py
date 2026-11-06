"""Fabricated Codex conversations exercise the real terminal plugin contract."""

from __future__ import annotations

from unittest.mock import AsyncMock, Mock

import pytest

import api
from plugins import REGISTRY
from plugins.base import enabled_plugin_ids
from plugins.codex.models import CodexSession
from plugins.codex.picker_rows import build_section
from plugins.codex.plugin import CodexPlugin

SESSION = CodexSession(
    id="fabricated-codex",
    path="/example/.codex/sessions/rollout-fabricated.jsonl",
    title="Build a toy calculator",
    cwd="/example/calculator",
    model="example-model",
    modified_at="2026-01-01T10:00:00Z",
    archived=True,
    source="WSL: Example",
)


def test_codex_is_optional_and_registered():
    assert "codex" in [plugin.id for plugin in REGISTRY]
    assert "codex" not in enabled_plugin_ids(REGISTRY, {})
    assert "codex" in enabled_plugin_ids(REGISTRY, {"plugins": [{"id": "codex", "enabled": True}]})


def test_picker_shows_source_archive_and_fabricated_title():
    section = build_section([SESSION], loading=False, error="")
    assert section.rows[0].header
    assert section.rows[1].item == SESSION
    rendered = str(section.rows[1].render(0).renderables[0])
    assert "Build a toy calculator" in rendered
    assert "Archived" in rendered
    assert "WSL: Example" in rendered


def test_picker_error_is_literal_text():
    section = build_section([], loading=False, error="unreadable [example]")
    assert section.rows[0].render(0).plain == "unreadable [example]"


@pytest.mark.asyncio
async def test_open_uses_codex_api_and_keeps_tools_and_token_totals(monkeypatch):
    host = Mock(view="picker", session_path="")
    host.present_session = AsyncMock()
    plugin = CodexPlugin(host)
    post = AsyncMock(
        return_value={
            "messages": [
                {"role": "user", "content": "Build a toy calculator"},
                {
                    "role": "claude",
                    "content": "Calculator ready",
                    "tool_call_count": 1,
                    "items": [
                        {
                            "item_type": "ToolCall",
                            "tool_name": "exec_command",
                            "tool_input": '{"cmd":"echo example"}',
                            "tool_result": "example",
                        }
                    ],
                },
            ],
            "ongoing": False,
            "input_tokens": 40,
            "output_tokens": 10,
            "cache_read_tokens": 5,
            "total_tokens": 50,
        }
    )
    monkeypatch.setattr(api, "post_json", post)

    await plugin._load(SESSION)

    post.assert_awaited_once_with("/api/codex/session", {"path": SESSION.path})
    opened = host.present_session.await_args.args[1]
    assert opened.path == SESSION.path
    assert opened.meta.cwd == "/example/calculator"
    assert opened.totals.total_tokens == 50
    assert opened.totals.cache_read_tokens == 5
    assert [message.role_label for message in opened.messages] == ["User", "Codex"]
    assert opened.messages[1].items[0].tool_result == "example"
    assert host.view == "list"
    assert await plugin.load_full_message(SESSION.path, 1) == opened.messages[1]
    assert await plugin.load_full_message("/other-session", 1) is None


@pytest.mark.asyncio
async def test_failed_open_reports_error_and_releases_picker(monkeypatch):
    host = Mock(view="picker")
    host.present_session = AsyncMock()
    plugin = CodexPlugin(host)
    monkeypatch.setattr(api, "post_json", AsyncMock(side_effect=RuntimeError("unreadable")))

    await plugin._load(SESSION)

    host.present_session.assert_not_awaited()
    host.notify.assert_called_once()
    assert "unreadable" in host.notify.call_args.args[0]
    host.set_picker_busy.assert_called_with(False)
    assert host.view == "picker"


@pytest.mark.asyncio
async def test_refresh_updates_the_open_codex_session(monkeypatch):
    host = Mock(view="list", session_path=SESSION.path)
    host.present_session = AsyncMock()
    plugin = CodexPlugin(host)
    post = AsyncMock(return_value={"messages": [{"role": "user", "content": "first"}]})
    monkeypatch.setattr(api, "post_json", post)
    await plugin._load(SESSION)
    post.return_value = {"messages": [{"role": "user", "content": "updated"}]}

    await plugin._refresh_open()

    opened = host.apply_session_update.call_args.args[0]
    assert opened.messages[0].content == "updated"
    assert host.present_session.await_count == 1


@pytest.mark.asyncio
async def test_refresh_cannot_overwrite_a_different_open_source(monkeypatch):
    host = Mock(view="list", session_path="/claude/session.jsonl")
    plugin = CodexPlugin(host)
    plugin._session = SESSION
    post = AsyncMock()
    monkeypatch.setattr(api, "post_json", post)

    await plugin._refresh_open()

    post.assert_not_awaited()
    host.apply_session_update.assert_not_called()
