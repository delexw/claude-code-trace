"""Tests for the session-source plugin contract: which plugins are on, and how
the shell composes and routes to them without knowing any of them by name."""

from __future__ import annotations

import pytest
from textual.widgets import Static

import api
import app as app_module
from data_types import SessionInfo
from plugins import REGISTRY
from plugins.base import (
    PickerRow,
    PickerSection,
    PluginKey,
    PluginSet,
    SourcePlugin,
    binding_for,
    enabled_plugin_ids,
)
from plugins.claude import ClaudePlugin
from widgets.source_picker import SourcePicker

SESSION = SessionInfo(
    session_id="sid-claude001", path="/p/s1.jsonl", mod_time="2026-09-20T10:00:00Z"
)


class FakePlugin(SourcePlugin):
    """A second source, standing in for one added later."""

    id = "fake"
    label = "Fake"
    KEYS = (PluginKey("x", "poke", "Poke"),)

    def __init__(self, host) -> None:
        super().__init__(host)
        self.opened: list[object] = []
        self.poked = 0
        self.started = 0
        self.events: list[object] = []

    def start(self) -> None:
        self.started += 1

    def picker_section(self) -> PickerSection:
        return PickerSection(
            rows=[
                PickerRow(render=lambda _frame: "Fake source", header=True),
                PickerRow(render=lambda _frame: "fake conversation", item="fake-1"),
            ]
        )

    def open(self, item: object) -> None:
        self.opened.append(item)

    def sse_handlers(self):
        return {"fake-event": self.events.append}

    def compose_status(self):
        yield Static("fake status", id="fake-status")

    def check_action(self, action: str, view: str) -> bool | None:
        return view == "picker"

    def action_poke(self) -> None:
        self.poked += 1


class TwoSourceApp(app_module.CCTraceApp):
    # Relative CSS paths resolve next to the subclass's own module (tests/);
    # the shell's layout is not what these tests check.
    CSS_PATH = None
    PLUGIN_REGISTRY = (ClaudePlugin, FakePlugin)
    BINDINGS = [binding_for(FakePlugin.id, key) for key in FakePlugin.KEYS]


class _RecordingSSE:
    """Stands in for SSEClient: records handlers instead of connecting."""

    last: _RecordingSSE | None = None

    def __init__(self, *_args, **_kwargs):
        self.handlers: dict[str, list] = {}
        _RecordingSSE.last = self

    def on(self, event, handler):
        self.handlers.setdefault(event, []).append(handler)

    def start(self):
        pass

    def stop(self):
        pass


@pytest.fixture(autouse=True)
def _stub_network(monkeypatch):
    monkeypatch.setattr(app_module, "SSEClient", _RecordingSSE)

    async def dirs():
        return ["/p"]

    async def discover(_dirs):
        return [SESSION]

    async def nothing(*_args, **_kwargs):
        return None

    async def empty():
        return []

    monkeypatch.setattr(api, "get_project_dirs", dirs)
    monkeypatch.setattr(api, "discover_sessions", discover)
    monkeypatch.setattr(api, "watch_picker", nothing)
    monkeypatch.setattr(api, "unwatch_picker", nothing)
    monkeypatch.setattr(api, "unwatch_session", nothing)
    monkeypatch.setattr(api, "list_efficiency_jobs", empty)
    monkeypatch.setattr(api, "list_efficiency_summaries", empty)


def _settings(monkeypatch, fake_enabled: bool) -> None:
    async def settings():
        return {"plugins": [{"id": "fake", "label": "Fake", "enabled": fake_enabled}]}

    monkeypatch.setattr(api, "get_settings", settings)


async def _settle(pilot) -> None:
    await pilot.app.workers.wait_for_complete()
    await pilot.pause()


def _picker_entries(app) -> list[tuple[str, object]]:
    picker = app.query_one("#picker", SourcePicker)
    return [picker.entry_at_raw_index(i) for i in picker.selectable_indices()]


# ---------------------------------------------------------------------------
# Which plugins are on
# ---------------------------------------------------------------------------


def test_the_registered_sources_in_picker_order():
    assert [plugin.id for plugin in REGISTRY] == ["claude", "antigravity"]


def test_a_backend_without_a_plugins_list_leaves_only_required_plugins_on():
    registry = (ClaudePlugin, FakePlugin)
    assert enabled_plugin_ids(registry, None) == {"claude"}
    assert enabled_plugin_ids(registry, {}) == {"claude"}
    assert enabled_plugin_ids(registry, {"plugins": "nonsense"}) == {"claude"}


def test_a_plugin_the_backend_reports_on_is_on():
    settings = {"plugins": [{"id": "fake", "enabled": True}]}
    assert enabled_plugin_ids((ClaudePlugin, FakePlugin), settings) == {"claude", "fake"}


def test_a_plugin_the_backend_reports_off_is_off():
    settings = {"plugins": [{"id": "fake", "enabled": False}, {"id": "claude", "enabled": True}]}
    assert enabled_plugin_ids((ClaudePlugin, FakePlugin), settings) == {"claude"}


def test_a_plugin_that_cannot_be_disabled_stays_on():
    settings = {"plugins": [{"id": "claude", "enabled": False}]}
    assert enabled_plugin_ids((ClaudePlugin,), settings) == {"claude"}


def test_plugin_set_starts_with_only_the_plugins_that_cannot_be_disabled():
    plugins = PluginSet((ClaudePlugin, FakePlugin), host=None)
    assert [p.id for p in plugins.enabled()] == ["claude"]
    assert plugins.get("fake") is None

    turned_on, turned_off = plugins.set_enabled({"claude", "fake"})
    assert [p.id for p in turned_on] == ["fake"]
    assert turned_off == []
    assert [p.id for p in plugins.enabled()] == ["claude", "fake"]

    turned_on, turned_off = plugins.set_enabled({"claude"})
    assert turned_on == []
    assert [p.id for p in turned_off] == ["fake"]


# ---------------------------------------------------------------------------
# The shell composes and routes to whichever plugins are on
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_an_enabled_plugin_adds_its_rows_after_claude_s(monkeypatch):
    _settings(monkeypatch, fake_enabled=True)
    async with TwoSourceApp().run_test() as pilot:
        await _settle(pilot)
        assert _picker_entries(pilot.app) == [("claude", SESSION), ("fake", "fake-1")]
        assert pilot.app.query_one("#fake-status").display is True
        assert pilot.app.plugins.get("fake").started == 1


@pytest.mark.asyncio
async def test_opening_a_row_goes_to_the_plugin_that_owns_it(monkeypatch):
    _settings(monkeypatch, fake_enabled=True)
    async with TwoSourceApp().run_test() as pilot:
        await _settle(pilot)
        picker = pilot.app.query_one("#picker", SourcePicker)
        picker.index = picker.selectable_indices()[-1]
        picker.focus()
        await pilot.press("enter")
        await pilot.pause()

        assert pilot.app.plugins.get("fake").opened == ["fake-1"]


@pytest.mark.asyncio
async def test_a_plugin_key_runs_that_plugin_s_action(monkeypatch):
    _settings(monkeypatch, fake_enabled=True)
    async with TwoSourceApp().run_test() as pilot:
        await _settle(pilot)
        pilot.app.query_one("#picker", SourcePicker).focus()
        await pilot.press("x")
        await pilot.pause()

        assert pilot.app.plugins.get("fake").poked == 1
        assert pilot.app.check_action("plugin", ("fake", "poke")) is True
        pilot.app.view = "list"
        assert pilot.app.check_action("plugin", ("fake", "poke")) is False


@pytest.mark.asyncio
async def test_a_disabled_plugin_leaves_no_trace(monkeypatch):
    _settings(monkeypatch, fake_enabled=False)
    async with TwoSourceApp().run_test() as pilot:
        await _settle(pilot)
        app = pilot.app
        fake = app.plugins.all()[1]

        assert _picker_entries(app) == [("claude", SESSION)]
        assert app.query_one("#fake-status").display is False
        assert app.check_action("plugin", ("fake", "poke")) is False
        assert fake.started == 0

        for handler in _RecordingSSE.last.handlers["fake-event"]:
            await handler({"ignored": True})
        assert fake.events == []


@pytest.mark.asyncio
async def test_an_enabled_plugin_receives_its_own_live_events(monkeypatch):
    _settings(monkeypatch, fake_enabled=True)
    async with TwoSourceApp().run_test() as pilot:
        await _settle(pilot)
        for handler in _RecordingSSE.last.handlers["fake-event"]:
            await handler({"n": 1})
        assert pilot.app.plugins.get("fake").events == [{"n": 1}]
