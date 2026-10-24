"""The Antigravity source: step conversion, picker rows, and the shell routing to it."""

from __future__ import annotations

import pytest

import api
import app as app_module
from data_types import SessionInfo
from plugins import REGISTRY
from plugins.antigravity.convert import to_display_message
from plugins.antigravity.models import BrainFolder, step_from_dict
from plugins.antigravity.picker_rows import build_section
from widgets.message_list import MessageList
from widgets.source_picker import SourcePicker

SESSION = SessionInfo(
    session_id="sid-claude001", path="/p/s1.jsonl", mod_time="2026-09-20T10:00:00Z"
)

FOLDERS = [
    {
        "uuid": "old-0000",
        "path": "/b/old",
        "mod_time_ms": 1000,
        "title": "Older chat",
        "flavor": "antigravity",
        "has_transcript": True,
    },
    {
        "uuid": "artifacts",
        "path": "/b/art",
        "mod_time_ms": 9000,
        "title": None,
        "flavor": "antigravity-ide",
        "has_transcript": False,
    },
    {
        "uuid": "new-1111",
        "path": "/b/new",
        "mod_time_ms": 5000,
        "title": None,
        "flavor": "antigravity-ide",
        "has_transcript": True,
    },
]

STEPS = [
    {
        "step_index": 0,
        "role": "user",
        "content": "fix the build",
        "created_at": None,
        "tool_calls": [],
        "thinking_text": None,
        "error_text": None,
        "duration_ms": 0,
    },
    {
        "step_index": 1,
        "role": "assistant",
        "content": "",
        "created_at": None,
        "tool_calls": [{"name": "grep", "summary": "find errors", "input": '{"q": 1}'}],
        "thinking_text": "look first",
        "error_text": None,
        "duration_ms": 1200,
    },
]


def _step(**overrides) -> dict:
    base = {
        "step_index": 3,
        "role": "system",
        "content": "text",
        "created_at": None,
        "tool_calls": [],
        "thinking_text": None,
        "error_text": None,
        "duration_ms": 0,
    }
    return {**base, **overrides}


# ---------------------------------------------------------------------------
# Conversion
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("role", "display_role", "label"),
    [
        ("user", "user", "User"),
        ("assistant", "claude", "Assistant"),
        ("checkpoint", "compact", "Checkpoint"),
        ("tool", "system", "MCP Tool"),
        ("file", "system", "View File"),
        ("search", "system", "Grep Search"),
        ("error", "system", "Error"),
        ("context", "system", "Context"),
        ("system", "system", "System"),
    ],
)
def test_each_step_role_maps_to_a_shared_role_and_keeps_its_own_name(role, display_role, label):
    msg = to_display_message(step_from_dict(_step(role=role)))
    assert msg.role == display_role
    assert msg.role_label == label
    assert msg.is_error is (role == "error")


def test_a_step_with_no_text_reads_from_its_thought_calls_and_error():
    msg = to_display_message(
        step_from_dict(
            _step(
                content="  ",
                thinking_text="weigh options",
                tool_calls=[
                    {"name": "view", "summary": "open a.ts", "input": "{}"},
                    {"name": "ls", "summary": "", "input": ""},
                ],
                error_text="boom",
            )
        )
    )
    assert msg.content == "weigh options\n\nview — open a.ts\n\nls\n\nboom"


def test_thinking_and_tool_calls_become_detail_items():
    msg = to_display_message(step_from_dict(STEPS[1]))
    assert [i.item_type for i in msg.items] == ["Thinking", "ToolCall"]
    assert msg.items[0].text == "look first"
    tool = msg.items[1]
    assert (tool.tool_name, tool.tool_summary, tool.tool_input) == (
        "grep",
        "find errors",
        '{"q": 1}',
    )
    assert (msg.thinking_count, msg.tool_call_count, msg.output_count) == (1, 1, 1)
    assert msg.duration_ms == 1200


# ---------------------------------------------------------------------------
# Picker rows
# ---------------------------------------------------------------------------


def test_rows_list_conversations_only_newest_first_with_a_title_fallback():
    section = build_section([BrainFolder(**f) for f in FOLDERS], loading=False, error="")
    items = [row.item for row in section.rows if row.item is not None]
    assert [f.uuid for f in items] == ["new-1111", "old-0000"]
    assert section.rows[0].header is True
    assert "Antigravity (2)" in str(section.rows[0].render(0))
    assert "new-1111" in str(section.rows[1].render(0).renderables[0])


def test_no_conversations_shows_a_notice_instead_of_rows():
    section = build_section([BrainFolder(**FOLDERS[1])], loading=False, error="")
    assert [row.item for row in section.rows] == [None]


# ---------------------------------------------------------------------------
# The shell with both sources
# ---------------------------------------------------------------------------


class _QuietSSE:
    def __init__(self, *_a, **_k):
        pass

    def on(self, *_a):
        pass

    def start(self):
        pass

    def stop(self):
        pass


class BothSourcesApp(app_module.CCTraceApp):
    CSS_PATH = None


@pytest.fixture(autouse=True)
def _stub_network(monkeypatch):
    monkeypatch.setattr(app_module, "SSEClient", _QuietSSE)

    async def dirs():
        return ["/p"]

    async def discover(_dirs):
        return [SESSION]

    async def nothing(*_a, **_k):
        return None

    async def empty():
        return []

    async def get_json(path):
        assert path == "/api/antigravity/folders"
        return FOLDERS

    posted: list = []

    async def post_json(path, body=None):
        posted.append((path, body))
        return {"messages": STEPS, "count": 2, "start": 0, "roles": [], "total_rows": 2}

    for name, fn in {
        "get_project_dirs": dirs,
        "discover_sessions": discover,
        "watch_picker": nothing,
        "unwatch_picker": nothing,
        "unwatch_session": nothing,
        "list_efficiency_jobs": empty,
        "list_efficiency_summaries": empty,
        "get_json": get_json,
        "post_json": post_json,
    }.items():
        monkeypatch.setattr(api, name, fn)
    return posted


def _settings(monkeypatch, enabled: bool) -> None:
    async def settings():
        return {"plugins": [{"id": "antigravity", "label": "Antigravity", "enabled": enabled}]}

    monkeypatch.setattr(api, "get_settings", settings)


async def _settle(pilot) -> None:
    await pilot.app.workers.wait_for_complete()
    await pilot.pause()


def _entries(app) -> list[tuple[str, object]]:
    picker = app.query_one("#picker", SourcePicker)
    return [picker.entry_at_raw_index(i) for i in picker.selectable_indices()]


def test_antigravity_is_registered_after_claude():
    assert [p.id for p in REGISTRY][-1] == "antigravity"


@pytest.mark.asyncio
async def test_enter_on_an_antigravity_row_shows_its_steps_in_the_shared_list(
    monkeypatch, _stub_network
):
    _settings(monkeypatch, enabled=True)
    async with BothSourcesApp().run_test() as pilot:
        await _settle(pilot)
        app = pilot.app
        assert [plugin for plugin, _ in _entries(app)] == ["claude", "antigravity", "antigravity"]

        picker = app.query_one("#picker", SourcePicker)
        picker.index = picker.selectable_indices()[1]
        picker.focus()
        await pilot.press("enter")
        await _settle(pilot)

        assert _stub_network == [("/api/antigravity/session", {"path": "/b/new"})]
        assert app.view == "list"
        assert app.session_path == "/b/new"
        assert [m.role_label for m in app.messages] == ["User", "Assistant"]
        assert len(app.query_one("#list", MessageList).children) == 2


@pytest.mark.asyncio
async def test_a_disabled_antigravity_lists_nothing(monkeypatch, _stub_network):
    _settings(monkeypatch, enabled=False)
    async with BothSourcesApp().run_test() as pilot:
        await _settle(pilot)
        assert _entries(pilot.app) == [("claude", SESSION)]
