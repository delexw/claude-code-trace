"""The contract between the TUI shell and a session-source plugin.

A plugin is one place sessions come from (Claude Code today). It lives in
``plugins/<id>/`` and contributes picker rows, key bindings, SSE handlers and
any extra widgets it needs; the shell (``app.CCTraceApp``) composes whatever
the enabled plugins provide and never branches on a plugin's id.
"""

from __future__ import annotations

import inspect
from abc import ABC, abstractmethod
from collections.abc import Awaitable, Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, ClassVar, Protocol

from rich.console import RenderableType
from textual.binding import Binding

from data_types import DisplayMessage, SessionMeta, SessionTotals

if TYPE_CHECKING:
    from textual.screen import Screen
    from textual.widget import Widget
    from textual.worker import Worker

SseHandler = Callable[[object], object]

# The shell's views. A plugin adds its own views next to these.
PICKER_VIEW = "picker"
LIST_VIEW = "list"
DETAIL_VIEW = "detail"


@dataclass
class PickerRow:
    """One row of the shared session picker.

    ``item`` is what the plugin gets back in ``open``; a row without one opens
    nothing (a notice such as "No sessions found"). ``header`` rows cannot be
    highlighted. ``render`` receives the picker's spinner frame so a row with
    ``animated`` set can show live activity.
    """

    render: Callable[[int], RenderableType]
    item: object | None = None
    header: bool = False
    animated: bool = False


@dataclass
class PickerSection:
    """A plugin's part of the picker. ``loading`` keeps the picker's loading
    indicator up while the plugin is still fetching its first list."""

    rows: list[PickerRow] = field(default_factory=list)
    loading: bool = False


@dataclass(frozen=True)
class OpenedSession:
    """A loaded session, ready for the shared message list and detail views."""

    path: str
    messages: list[DisplayMessage]
    ongoing: bool
    meta: SessionMeta
    totals: SessionTotals


@dataclass(frozen=True)
class PluginKey:
    """A key binding that only applies while its plugin is enabled.

    The shell turns it into ``Binding(key, "plugin('<id>', '<action>')")`` and
    routes the action to ``SourcePlugin.action_<action>``.
    """

    key: str
    action: str
    description: str
    show: bool = True


class PluginHost(Protocol):
    """What a plugin may use from the shell."""

    view: str
    session_path: str
    messages: list[DisplayMessage]
    ongoing: bool
    meta: SessionMeta
    totals: SessionTotals

    def sync_picker(self) -> None: ...
    def refresh_picker_rows(self, plugin_id: str, items: Iterable[object]) -> None: ...
    def set_picker_busy(self, busy: bool) -> None: ...
    async def present_session(self, plugin: SourcePlugin, session: OpenedSession) -> None: ...
    def apply_session_update(self, session: OpenedSession) -> None: ...
    def sync_view(self) -> None: ...
    def focus_content(self) -> None: ...
    def highlighted_picker_item(self, plugin_id: str) -> object | None: ...
    def notify(self, message: str, **kwargs) -> None: ...
    def run_worker(self, work, **kwargs) -> Worker: ...
    def push_screen(self, screen: Screen, *args, **kwargs) -> object: ...
    def push_screen_wait(self, screen: Screen) -> Awaitable[object]: ...
    def copy_to_clipboard(self, text: str) -> None: ...


class SourcePlugin(ABC):
    """Base class for a session source. Subclasses live in ``plugins/<id>/``."""

    id: ClassVar[str]
    label: ClassVar[str]
    # False for a source the app cannot run without.
    can_disable: ClassVar[bool] = True
    KEYS: ClassVar[Sequence[PluginKey]] = ()

    def __init__(self, host: PluginHost) -> None:
        self.host = host

    # ---- Lifecycle ----

    def start(self) -> None:
        """Called once this plugin is enabled; starts its own workers."""
        return

    async def stop(self) -> None:
        """Called when the app shuts down."""
        return

    def refresh(self) -> None:
        """``r``: re-read this source's sessions."""
        return

    def sse_handlers(self) -> Mapping[str, SseHandler]:
        """Backend ``/api/events`` events this plugin listens to."""
        return {}

    # ---- Picker ----

    @abstractmethod
    def picker_section(self) -> PickerSection:
        """This plugin's rows for the shared picker."""

    @abstractmethod
    def open(self, item: object) -> None:
        """Open a picker item (load it, then ``host.present_session``)."""

    async def load_full_message(self, path: str, index: int) -> DisplayMessage | None:
        """The full body of one message of a session this plugin opened."""
        return None

    # ---- Extra widgets ----

    def compose_sidebar(self) -> Iterable[Widget]:
        """Widgets shown left of the content pane."""
        return ()

    def compose_views(self) -> Iterable[Widget]:
        """Extra content views; each widget's id is its view name."""
        return ()

    def compose_status(self) -> Iterable[Widget]:
        """Widgets docked above the footer."""
        return ()

    def sync_sidebar(self) -> None:
        """Push this plugin's state into its sidebar widgets."""
        return

    def sync_view(self, view: str) -> None:
        """Push this plugin's state into one of its own views."""
        return

    def focus_view(self, view: str) -> None:
        """Move keyboard focus into one of this plugin's own views."""
        return

    # ---- Keys ----

    def check_action(self, action: str, view: str) -> bool | None:
        """Whether one of this plugin's ``KEYS`` applies in ``view``."""
        return True

    def run_action(self, action: str) -> None:
        method = getattr(self, f"action_{action}", None)
        if not callable(method):
            return
        result = method()
        if inspect.iscoroutine(result):
            self.host.run_worker(result, exclusive=False)


def binding_for(plugin_id: str, key: PluginKey) -> Binding:
    return Binding(
        key.key, f"plugin({plugin_id!r}, {key.action!r})", key.description, show=key.show
    )


def enabled_plugin_ids(
    registry: Sequence[type[SourcePlugin]], settings: Mapping[str, object] | None
) -> set[str]:
    """Which registered plugins are on, from ``GET /api/settings``.

    A plugin that cannot be disabled is always on; any other plugin is on only
    when the backend reports it enabled, so an older backend that does not know
    about it never has its routes called.
    """
    reported = (settings or {}).get("plugins")
    switched_on = (
        {
            entry.get("id")
            for entry in reported
            if isinstance(entry, Mapping) and entry.get("enabled") is True
        }
        if isinstance(reported, list)
        else set()
    )
    return {plugin.id for plugin in registry if not plugin.can_disable or plugin.id in switched_on}


class PluginSet:
    """The registered plugins, instantiated for one app, and which are on.

    Until the backend says otherwise only plugins that cannot be disabled are
    on, so an optional source never flashes up and then disappears.
    """

    def __init__(self, registry: Sequence[type[SourcePlugin]], host: PluginHost) -> None:
        self._plugins: dict[str, SourcePlugin] = {cls.id: cls(host) for cls in registry}
        self._enabled: set[str] = {cls.id for cls in registry if not cls.can_disable}

    def all(self) -> list[SourcePlugin]:
        return list(self._plugins.values())

    def enabled(self) -> list[SourcePlugin]:
        return [p for pid, p in self._plugins.items() if pid in self._enabled]

    def get(self, plugin_id: str) -> SourcePlugin | None:
        """The plugin with this id, or None when it is unknown or disabled."""
        return self._plugins.get(plugin_id) if plugin_id in self._enabled else None

    def is_enabled(self, plugin_id: str) -> bool:
        return plugin_id in self._enabled

    def set_enabled(self, ids: set[str]) -> tuple[list[SourcePlugin], list[SourcePlugin]]:
        """Switch to exactly ``ids``; returns the plugins turned on and off."""
        known = ids & self._plugins.keys()
        turned_on = [p for pid, p in self._plugins.items() if pid in known - self._enabled]
        turned_off = [p for pid, p in self._plugins.items() if pid in self._enabled - known]
        self._enabled = known
        return turned_on, turned_off
