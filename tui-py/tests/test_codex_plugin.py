"""Fabricated Codex conversations exercise the real terminal plugin contract."""

from __future__ import annotations

from dataclasses import replace
from unittest.mock import AsyncMock, Mock

import pytest

import api
from plugins import REGISTRY
from plugins.base import enabled_plugin_ids
from plugins.codex.models import CodexSession, session_from_dict
from plugins.codex.picker_rows import build_section
from plugins.codex.plugin import CodexPlugin
from plugins.codex.projects import codex_project_key
from plugins.codex.resume_command import resume_command

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
    row = next(row for row in section.rows if row.item == SESSION)
    rendered = str(row.render(0).renderables[0])
    assert "Build a toy calculator" in rendered
    assert "Archived" in rendered
    assert "WSL: Example" in rendered


def test_picker_error_is_literal_text():
    section = build_section([], loading=False, error="unreadable [example]")
    assert section.rows[0].render(0).plain == "unreadable [example]"


@pytest.mark.parametrize(
    "cost, expected", [(1.25, "~$1.25"), (0.0, "~$0.00"), (None, "Cost unavailable")]
)
def test_picker_displays_estimated_cost_without_treating_unknown_as_zero(cost, expected):
    session = session_from_dict({"id": "fabricated", "cost_usd": cost, "total_tokens": 1200})
    section = build_section([session], loading=False, error="")
    row = next(row for row in section.rows if row.item is not None)
    assert expected in str(row.render(0).renderables[0])


def test_picker_total_marks_a_partial_estimate():
    sessions = [session_from_dict({"cost_usd": 1.25}), session_from_dict({"cost_usd": None})]
    section = build_section(sessions, loading=False, error="")
    assert "~$1.25+" in section.rows[0].render(0).plain


def test_picker_retains_partial_session_subtotals_and_marks_the_aggregate():
    session = replace(SESSION, cost_usd=1.25, cost_partial=True)
    section = build_section([session], loading=False, error="")
    assert "~$1.25+" in section.rows[0].render(0).plain
    row = next(row for row in section.rows if row.item == session)
    assert "~$1.25+" in str(row.render(0).renderables[0])


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
            "cost_usd": 1.25,
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
    assert opened.totals.cost_usd == 1.25
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


def test_project_selection_filters_rows_and_persists_when_sessions_refresh():
    host = Mock(view="list")
    plugin = CodexPlugin(host)
    second = replace(SESSION, id="second", cwd="/example/another")
    plugin.sessions = [SESSION, second]
    plugin.loading = False
    plugin._select_project(codex_project_key(second))
    assert host.view == "picker"
    assert [row.item for row in plugin.picker_section().rows if row.item] == [second]
    plugin.sessions = [second, SESSION]
    assert [row.item for row in plugin.picker_section().rows if row.item] == [second]
    plugin._select_project(None)
    assert len([row for row in plugin.picker_section().rows if row.item]) == 2


@pytest.mark.asyncio
async def test_load_keeps_metadata_context_unknown_cost_and_nested_codex_roles(monkeypatch):
    host = Mock(view="picker", session_path="")
    host.present_session = AsyncMock()
    plugin = CodexPlugin(host)
    monkeypatch.setattr(
        api,
        "post_json",
        AsyncMock(
            return_value={
                "messages": [
                    {
                        "role": "claude",
                        "items": [
                            {
                                "item_type": "Subagent",
                                "subagent_messages": [
                                    {"role": "claude", "content": "Fabricated worker"}
                                ],
                            }
                        ],
                    }
                ],
                "model": "example-latest",
                "git_branch": "feature/example",
                "permission_mode": "on-request",
                "sandbox_policy": "workspace-write",
                "duration_ms": 12000,
                "context_tokens": 25000,
                "context_window": 100000,
                "cost_usd": None,
            }
        ),
    )
    await plugin._load(SESSION)
    opened = host.present_session.await_args.args[1]
    assert opened.meta.session_id == SESSION.id
    assert opened.meta.git_branch == "feature/example"
    assert opened.meta.permission_mode == "on-request"
    assert opened.meta.sandbox_policy == "workspace-write"
    assert opened.meta.duration_ms == 12000
    assert opened.meta.context_tokens == 25000
    assert opened.meta.context_window == 100000
    assert opened.totals.model == "example-latest"
    assert opened.totals.cost_usd is None
    assert opened.totals.estimated_cost
    assert opened.messages[0].items[0].subagent_messages[0].role_label == "Codex"


@pytest.mark.asyncio
async def test_live_poll_reads_metadata_then_reloads_a_changed_revision(
    monkeypatch,
):
    host = Mock(view="list", session_path=SESSION.path)
    host.present_session = AsyncMock()
    plugin = CodexPlugin(host)
    post = AsyncMock(
        return_value={
            "revision": "first",
            "count": 2,
            "messages": [
                {"role": "user", "content": "Fabricated request"},
                {"role": "claude", "content": "First answer"},
            ],
            "cost_usd": 1.0,
        }
    )
    monkeypatch.setattr(api, "post_json", post)
    await plugin._load(SESSION)
    plugin._started = True
    post.reset_mock()
    post.return_value = {
        "revision": "first",
        "count": 2,
        "messages": [],
        "cost_usd": 1.25,
        "ongoing": True,
    }
    await plugin._poll_once()
    post.assert_awaited_once_with("/api/codex/session", {"path": SESSION.path, "limit": 0})
    assert host.apply_session_update.call_args.args[0].messages[0].content == "Fabricated request"
    assert host.apply_session_update.call_args.args[0].totals.cost_usd == 1.25
    post.reset_mock()
    post.side_effect = [
        {"revision": "second", "count": 3},
        {
            "revision": "second",
            "count": 3,
            "messages": [
                {"role": "user", "content": "Revised fabricated request"},
                {"role": "claude", "content": "Updated answer"},
                {"role": "user", "content": "Follow up"},
            ],
        },
    ]
    await plugin._poll_once()
    assert post.await_args_list[1].args[1] == {"path": SESSION.path}
    assert [
        message.content for message in host.apply_session_update.call_args.args[0].messages
    ] == [
        "Revised fabricated request",
        "Updated answer",
        "Follow up",
    ]
    assert plugin._messages[1].role_label == "Codex"


@pytest.mark.asyncio
async def test_live_poll_skips_disabled_and_other_sources(monkeypatch):
    host = Mock(view="list", session_path="/another/source")
    plugin = CodexPlugin(host)
    plugin._session = SESSION
    post = AsyncMock()
    monkeypatch.setattr(api, "post_json", post)
    await plugin._poll_once()
    plugin._started = True
    await plugin._poll_once()
    post.assert_not_awaited()


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "replacement",
    [
        [{"role": "user", "content": "Rewritten request"}],
        [
            {
                "role": "claude",
                "items": [
                    {
                        "item_type": "Subagent",
                        "subagent_messages": [{"role": "claude", "content": "Updated worker"}],
                    }
                ],
            },
            {"role": "user", "content": "Unchanged follow up"},
        ],
    ],
)
async def test_live_poll_replaces_truncated_or_earlier_worker_messages(monkeypatch, replacement):
    host = Mock(view="list", session_path=SESSION.path)
    host.present_session = AsyncMock()
    plugin = CodexPlugin(host)
    post = AsyncMock(
        return_value={
            "revision": "initial",
            "messages": [
                {
                    "role": "claude",
                    "items": [
                        {
                            "item_type": "Subagent",
                            "subagent_messages": [{"role": "claude", "content": "Old worker"}],
                        }
                    ],
                },
                {"role": "user", "content": "Unchanged follow up"},
            ],
        }
    )
    monkeypatch.setattr(api, "post_json", post)
    await plugin._load(SESSION)
    plugin._started = True
    post.reset_mock()
    post.side_effect = [
        {"revision": "dependency-changed", "count": len(replacement)},
        {"revision": "dependency-changed", "count": len(replacement), "messages": replacement},
    ]
    await plugin._poll_once()
    assert post.await_args_list[1].args[1] == {"path": SESSION.path}
    messages = host.apply_session_update.call_args.args[0].messages
    assert len(messages) == len(replacement)
    if len(replacement) == 1:
        assert messages[0].content == "Rewritten request"
    else:
        assert messages[0].items[0].subagent_messages[0].content == "Updated worker"
        assert messages[0].items[0].subagent_messages[0].role_label == "Codex"


def test_resume_quotes_shell_arguments_and_applies_only_to_active_codex_session():
    posix = replace(SESSION, cwd="/example/it's a demo", id="sample id")
    assert resume_command(posix) == "cd '/example/it'\"'\"'s a demo' && codex resume 'sample id'"
    windows = replace(SESSION, cwd="D:\\Example\\it's a demo")
    assert (
        resume_command(windows)
        == "Set-Location -LiteralPath 'D:\\Example\\it''s a demo'; codex resume 'fabricated-codex'"
    )
    host = Mock(view="list", session_path=SESSION.path)
    plugin = CodexPlugin(host)
    plugin._session = SESSION
    assert plugin.check_action("copy_resume", "list")
    assert not plugin.check_action("copy_resume", "picker")
    plugin.action_copy_resume()
    host.copy_to_clipboard.assert_called_once_with(resume_command(SESSION))
    host.session_path = "/another/source"
    assert not plugin.check_action("copy_resume", "list")
