"""Main Textual application for Claude Code Trace TUI.

Architecture:
- The app is a shell: it owns the shared views (picker, message list,
  detail) and the session open in them. Everything about one source of
  sessions — listing, loading, its own keys, views and live events — lives in
  a plugin under ``plugins/<id>/`` (see ``plugins/base.py``).
- BINDINGS + action methods handle view-switching and expand/collapse.
- ListView.Selected / ListView.Highlighted / Tree.NodeSelected events
  route to handlers instead of on_key.
- No event.stop() in on_key; native widgets handle their own j/k/Enter.
"""

from __future__ import annotations

import asyncio
import contextlib
import inspect

from textual import events
from textual.actions import parse as parse_action
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import ContentSwitcher, Footer, Header, ListView

import api as api_client
import auth
from data_types import (
    DisplayMessage,
    SessionMeta,
    SessionTotals,
)
from plugins import REGISTRY
from plugins.base import (
    LIST_VIEW,
    PICKER_VIEW,
    OpenedSession,
    PluginSet,
    SourcePlugin,
    SseHandler,
    binding_for,
    enabled_plugin_ids,
)
from sse import SSEClient
from widgets.detail_view import DetailView
from widgets.message_list import MessageList
from widgets.source_picker import SourcePicker

API_BASE = "http://127.0.0.1:11423"


class SidebarResizer(Widget):
    """1-column drag handle between the sidebar and the content pane."""

    DEFAULT_CSS = """
    SidebarResizer {
        width: 1;
        height: 100%;
        background: $border;
    }
    SidebarResizer:hover {
        background: $accent;
    }
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._dragging = False

    def render(self) -> str:
        return ""

    def on_mouse_down(self, event: events.MouseDown) -> None:
        self._dragging = True
        self.capture_mouse()
        event.stop()

    def on_mouse_move(self, event: events.MouseMove) -> None:
        if self._dragging:
            new_width = max(15, min(60, event.screen_x))
            with contextlib.suppress(Exception):
                siblings = list(self.parent.children)
                siblings[siblings.index(self) - 1].styles.width = new_width

    def on_mouse_up(self, event: events.MouseUp) -> None:
        if self._dragging:
            self._dragging = False
            self.release_mouse()
        event.stop()


class CCTraceApp(App):
    """Claude Code Trace — terminal session viewer."""

    CSS_PATH = "cctrace.tcss"
    TITLE = "cctrace"

    # ---- Global bindings (priority=True intercepts before focused widgets) ----
    # Global bindings — always visible in Footer regardless of focused widget.
    # j/k/enter target App-level delegation actions that forward to the
    # focused widget; that keeps them visible in the Footer on every page
    # while still driving the focused list's cursor.
    BINDINGS = [
        Binding("k", "focused_cursor_up", "↑", show=True),
        Binding("j", "focused_cursor_down", "↓", show=True),
        Binding("enter", "focused_select_cursor", "Open", show=True),
        Binding("q", "back_or_quit", "Quit/Back", priority=True),
        Binding("escape", "back_or_quit", "Back", show=False, priority=True),
        Binding("tab", "toggle_expand", "Expand/Detail", show=True, priority=True),
        Binding("e", "expand_all", "Expand All", show=True, priority=True),
        Binding("c", "collapse_all", "Collapse All", show=True, priority=True),
        Binding("g", "jump_first", "First", show=True, priority=True),
        Binding("G", "jump_last", "Last", show=True, priority=True),
        Binding("u", "scroll_up", "Scroll↑", show=True, priority=True),
        Binding("d", "d_action", "Scroll↓", show=True, priority=True),
        Binding("r", "refresh", "Refresh", show=True, priority=True),
        Binding("h", "focus_sidebar", "◀ Sidebar", show=True, priority=True),
        Binding("l", "focus_content", "Content ▶", show=False, priority=True),
        # Each plugin's own keys, routed to it by action_plugin; whether they
        # apply in the current view is the plugin's call (see check_action).
        *(binding_for(plugin.id, key) for plugin in REGISTRY for key in plugin.KEYS),
    ]

    # The sources this app shows. A subclass may swap in another registry (and
    # add its plugins' keys to BINDINGS, which Textual merges with these).
    PLUGIN_REGISTRY = REGISTRY

    # Derived from BINDINGS so a key added above cannot be forgotten here —
    # see check_action, which disarms exactly these while a modal is open.
    OWN_ACTIONS = frozenset(parse_action(binding.action)[1] for binding in BINDINGS)

    # ---- State ----
    view: reactive[str] = reactive("picker")

    # Session
    session_path: reactive[str] = reactive("")
    messages: reactive[list] = reactive(list)
    ongoing: reactive[bool] = reactive(False)
    meta: reactive[SessionMeta] = reactive(SessionMeta)
    totals: reactive[SessionTotals] = reactive(SessionTotals)

    # List view
    expanded_messages: reactive[set] = reactive(set)

    # Detail view — tracks which item is currently highlighted
    _detail_item_idx: int = 0
    expanded_items: reactive[set] = reactive(set)

    # Subagent drill-down
    subagent_item: reactive = reactive(None)
    subagent_detail_msg: reactive = reactive(None)
    # When drilling into a subagent message list, expanded tracking resets
    _subagent_expanded_msgs: set[int]
    _subagent_detail_item_idx: int

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._sse: SSEClient | None = None
        self._subagent_expanded_msgs: set[int] = set()
        self._subagent_detail_item_idx: int = 0
        self.plugins = PluginSet(self.PLUGIN_REGISTRY, self)
        # The plugin that opened the session shown in the list/detail views.
        self._session_plugin: SourcePlugin | None = None
        # Extra views by name, and the plugin each belongs to.
        self._view_owners: dict[str, SourcePlugin] = {}
        # Every widget a plugin added, so it can be hidden while it is off.
        self._plugin_widgets: dict[str, list[Widget]] = {}
        self._sidebar_widgets: list[Widget] = []

    # ----------------------------------------------------------------
    # Layout
    # ----------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="main-area"):
            self._sidebar_widgets = self._take_widgets("compose_sidebar")
            yield from self._sidebar_widgets
            if self._sidebar_widgets:
                yield SidebarResizer(id="sidebar-resizer")
            with ContentSwitcher(initial="picker", id="content-switcher"):
                yield SourcePicker(id="picker")
                yield MessageList(id="list")
                yield DetailView(id="detail")
                for plugin in self.plugins.all():
                    for view in self._take_widgets("compose_views", plugin):
                        self._view_owners[view.id] = plugin
                        yield view
        yield from self._take_widgets("compose_status")
        yield Footer()

    def _take_widgets(self, contribution: str, only: SourcePlugin | None = None) -> list[Widget]:
        """Collect one kind of widget (a ``SourcePlugin.compose_*`` method) from
        every plugin, or just ``only``; hidden for plugins that are off."""
        widgets: list[Widget] = []
        for plugin in [only] if only else self.plugins.all():
            for widget in getattr(plugin, contribution)():
                widget.display = self.plugins.is_enabled(plugin.id)
                self._plugin_widgets.setdefault(plugin.id, []).append(widget)
                widgets.append(widget)
        return widgets

    # ----------------------------------------------------------------
    # Lifecycle
    # ----------------------------------------------------------------

    def on_mount(self) -> None:
        self.title = "cctrace"
        for plugin in self.plugins.enabled():
            plugin.start()
        self.run_worker(self._load_enabled_plugins(), exclusive=False, name="plugins")
        self._start_sse()

    def _start_sse(self) -> None:
        # `auth.auth_headers` is re-evaluated on every (re)connect so a credential
        # reissued from the Settings UI is picked up without restarting the TUI.
        self._sse = SSEClient(f"{API_BASE}/api/events", headers=auth.auth_headers)
        for plugin in self.plugins.all():
            for event, handler in plugin.sse_handlers().items():
                self._sse.on(event, self._while_enabled(plugin, handler))
        self._sse.start()

    def _while_enabled(self, plugin: SourcePlugin, handler: SseHandler) -> SseHandler:
        async def handle(payload) -> None:
            if self.plugins.is_enabled(plugin.id):
                result = handler(payload)
                if inspect.isawaitable(result):
                    await result

        return handle

    async def _load_enabled_plugins(self) -> None:
        """Ask the backend which sources are switched on."""
        try:
            settings = await api_client.get_settings()
        except Exception:
            settings = None
        self._apply_enabled_plugins(enabled_plugin_ids(self.PLUGIN_REGISTRY, settings))

    def _apply_enabled_plugins(self, ids: set[str]) -> None:
        turned_on, turned_off = self.plugins.set_enabled(ids)
        if not turned_on and not turned_off:
            return
        for plugin in [*turned_on, *turned_off]:
            for widget in self._plugin_widgets.get(plugin.id, []):
                widget.display = self.plugins.is_enabled(plugin.id)
        for plugin in turned_on:
            plugin.start()
        if self.view == PICKER_VIEW:
            self.sync_picker()

    async def on_unmount(self) -> None:
        if self._sse:
            self._sse.stop()
        # Best-effort cleanup with a short timeout; don't block exit.
        with contextlib.suppress(Exception):
            await asyncio.wait_for(self._cleanup(), timeout=1.0)

    async def _cleanup(self) -> None:
        for plugin in self.plugins.enabled():
            await plugin.stop()

    # ----------------------------------------------------------------
    # Plugin host API (see plugins.base.PluginHost)
    # ----------------------------------------------------------------

    async def present_session(self, plugin: SourcePlugin, session: OpenedSession) -> None:
        """Load ``session`` into the message list. Fully populates the list
        (awaited) so the caller can flip the view only once it is ready."""
        self._session_plugin = plugin
        self.session_path = session.path
        # Set everything except `messages` first — `messages` triggers
        # watch_messages, but that watcher early-returns while view !=
        # "list", so the SyncMessageList worker doesn't race us.
        self.ongoing = session.ongoing
        self.meta = session.meta
        self.totals = session.totals
        self.expanded_messages = set()
        self._detail_full_message = None
        self.messages = session.messages
        with contextlib.suppress(Exception):
            ml = self.query_one("#list", MessageList)
            await ml.populate(
                messages=session.messages,
                expanded_set=self.expanded_messages,
                ongoing=session.ongoing,
            )

    def apply_session_update(self, session: OpenedSession) -> None:
        """A fresh copy of the open session arrived (a live update)."""
        self.ongoing = session.ongoing
        self.meta = session.meta
        self.totals = session.totals
        self.messages = session.messages
        self._sync_all_widgets()

    def set_picker_busy(self, busy: bool) -> None:
        with contextlib.suppress(Exception):
            self.query_one("#picker", SourcePicker).loading = busy

    def sync_picker(self) -> None:
        self._sync_session_picker()

    def refresh_picker_rows(self, plugin_id: str, items) -> None:
        with contextlib.suppress(Exception):
            self.query_one("#picker", SourcePicker).refresh_rows(plugin_id, items)

    def sync_view(self) -> None:
        self._sync_content_view()

    def focus_content(self) -> None:
        self._focus_active_widget()

    def highlighted_picker_item(self, plugin_id: str) -> object | None:
        try:
            picker = self.query_one("#picker", SourcePicker)
            if picker.index is None:
                return None
            entry = picker.entry_at_raw_index(picker.index)
        except Exception:
            return None
        if entry is None or entry[0] != plugin_id:
            return None
        return entry[1]

    # ----------------------------------------------------------------
    # Widget event handlers — ListView, Tree
    # ----------------------------------------------------------------

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle Enter on a ListView item."""
        lv_id = event.list_view.id

        if lv_id == "picker":
            # event.index is the raw ListView node index (including disabled header rows)
            picker = self.query_one("#picker", SourcePicker)
            entry = picker.entry_at_raw_index(event.index)
            plugin = self.plugins.get(entry[0]) if entry is not None else None
            if entry is not None and plugin is not None:
                # The plugin shows a LoadingIndicator on the picker, fully
                # populates the MessageList, *then* flips the view — so the
                # user never lands on a half-built list pane.
                self._detail_item_idx = 0
                self.expanded_items = set()
                self.subagent_item = None
                self.subagent_detail_msg = None
                plugin.open(entry[1])

        elif lv_id == "list":
            # event.index is the direct message index (MessageList has no header rows)
            raw_idx = event.index

            if self.subagent_item and not self.subagent_detail_msg:
                # Enter on subagent message list → open subagent detail
                msgs = self.subagent_item.subagent_messages
                if 0 <= raw_idx < len(msgs):
                    self.subagent_detail_msg = msgs[raw_idx]
                    self._subagent_detail_item_idx = 0
                    self.expanded_items = set()
                    self._sync_detail_view()
                    self.view = "detail"
            else:
                # Enter on main message list → open detail
                if self.messages and 0 <= raw_idx < len(self.messages):
                    self._detail_item_idx = 0
                    self.expanded_items = set()
                    self.subagent_item = None
                    self.subagent_detail_msg = None
                    self._sync_detail_view_for_message_index(raw_idx)
                    self.view = "detail"

        elif lv_id == "items-list":
            # Enter on a detail-view item → toggle expand, or drill into subagent
            if event.index is not None:
                self._handle_detail_enter(event.index)

    def on_list_view_highlighted(self, _event: ListView.Highlighted) -> None:
        """No-op — widgets manage their own highlight; Footer renders keybinds."""

    # ----------------------------------------------------------------
    # Action methods (from BINDINGS)
    # ----------------------------------------------------------------

    def _delegate_to_focused(self, action: str) -> None:
        """Call action_<action> on the currently focused widget (if any).

        Used so j/k/enter can live as App-level bindings (always visible in
        the Footer) while still driving the focused list's own cursor.
        """
        focused = self.focused
        if focused is None:
            return
        with contextlib.suppress(Exception):
            method = getattr(focused, f"action_{action}", None)
            if callable(method):
                result = method()
                if inspect.iscoroutine(result):
                    self.run_worker(result, exclusive=False)

    def action_focused_cursor_up(self) -> None:
        self._delegate_to_focused("cursor_up")

    def action_focused_cursor_down(self) -> None:
        self._delegate_to_focused("cursor_down")

    def action_focused_select_cursor(self) -> None:
        self._delegate_to_focused("select_cursor")

    def action_back_or_quit(self) -> None:
        match self.view:
            case "picker":
                self.exit()
            case "list":
                if self.subagent_item and not self.subagent_detail_msg:
                    # Back from subagent message list → back to detail.
                    # Set view first so watch_view renders detail immediately;
                    # clearing subagent_item after avoids a flash of the main list.
                    self.view = "detail"
                    self.subagent_item = None
                else:
                    self.view = "picker"
            case "detail":
                if self.subagent_detail_msg:
                    self.subagent_detail_msg = None
                    self._sync_detail_view()
                elif self.subagent_item:
                    # Go back to subagent message list (shown in "list" pane)
                    self._sync_subagent_message_list()
                    self.view = "list"
                else:
                    # Leaving the top-level detail view entirely — release the
                    # full (heavy-body) message it held.
                    self._detail_full_message = None
                    self.view = "list"
            case view if view in self._view_owners:
                self.view = LIST_VIEW

    def action_toggle_expand(self) -> None:
        """Tab: in list view → open detail; in detail view → toggle expand current item."""
        if self.view == "list":
            idx = self._current_list_index()
            if idx is None:
                return
            if self.subagent_item and not self.subagent_detail_msg:
                msgs = self.subagent_item.subagent_messages
                if 0 <= idx < len(msgs):
                    self.subagent_detail_msg = msgs[idx]
                    self._subagent_detail_item_idx = 0
                    self.expanded_items = set()
                    self._sync_detail_view()
                    self.view = "detail"
            else:
                if self.messages and 0 <= idx < len(self.messages):
                    self._detail_item_idx = 0
                    self.expanded_items = set()
                    self.subagent_item = None
                    self.subagent_detail_msg = None
                    self._sync_detail_view_for_message_index(idx)
                    self.view = "detail"
        elif self.view == "detail":
            idx = self._current_detail_item_index()
            if idx is not None:
                try:
                    dv = self.query_one("#detail", DetailView)
                    dv.toggle_item(idx)
                except Exception:
                    pass

    def action_expand_all(self) -> None:
        if self.view == "list":
            n = len(self.messages)
            self.expanded_messages = set(range(n))
            self._sync_message_list()
        elif self.view == "detail":
            with contextlib.suppress(Exception):
                self.query_one("#detail", DetailView).expand_all()

    def action_collapse_all(self) -> None:
        if self.view == "list":
            self.expanded_messages = set()
            self._sync_message_list()
        elif self.view == "detail":
            with contextlib.suppress(Exception):
                self.query_one("#detail", DetailView).collapse_all()

    def action_refresh(self) -> None:
        """r: re-read every enabled source."""
        for plugin in self.plugins.enabled():
            plugin.refresh()

    def action_plugin(self, plugin_id: str, action: str) -> None:
        """Run one of a plugin's own key actions (see ``PluginKey``)."""
        plugin = self.plugins.get(plugin_id)
        if plugin is not None:
            plugin.run_action(action)

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        """A plugin's keys apply only while it is enabled, and only in the
        views the plugin says they do — keeping them off the global footer,
        which already overflows on narrow terminals.

        While a modal screen (the privacy notice, the analytics settings) is
        open it owns the keyboard: without this the app's priority bindings
        would steal Escape and swallow characters typed into its inputs.
        Only this app's own actions are disarmed — Textual's app-level
        actions (`focus_next`, `focus_previous`, …) are what a modal's own
        Tab bindings run, so blanking those out leaves the modal navigable
        by mouse only.
        """
        if len(self.screen_stack) > 1 and action in self.OWN_ACTIONS:
            return False
        if action == "plugin":
            plugin_id, plugin_action = parameters
            plugin = self.plugins.get(str(plugin_id))
            if plugin is None:
                return False
            return plugin.check_action(str(plugin_action), self.view)
        return True

    def action_d_action(self) -> None:
        """d: scroll down in current view, or show debug log from list view."""
        match self.view:
            case "picker":
                with contextlib.suppress(Exception):
                    self.query_one("#picker", SourcePicker).scroll_down(amount=5, animate=False)
            case "list":
                with contextlib.suppress(Exception):
                    self.query_one("#list", MessageList).scroll_down(amount=5, animate=False)
            case "detail":
                with contextlib.suppress(Exception):
                    dv = self.query_one("#detail", DetailView)
                    dv.query_one("#items-scroll").scroll_down(amount=5, animate=False)

    def action_jump_first(self) -> None:
        """g: jump to first item."""
        if self.view == "picker":
            try:
                lv = self.query_one("#picker", SourcePicker)
                selectable = lv.selectable_indices()
                if selectable:
                    lv.index = selectable[0]
            except Exception:
                pass
        elif self.view == "list":
            with contextlib.suppress(Exception):
                self.query_one("#list", MessageList).index = 0
        elif self.view == "detail":
            try:
                dv = self.query_one("#detail", DetailView)
                dv.focus_item(0)
            except Exception:
                pass

    def action_jump_last(self) -> None:
        """G: jump to last item."""
        if self.view == "picker":
            try:
                lv = self.query_one("#picker", SourcePicker)
                selectable = lv.selectable_indices()
                if selectable:
                    lv.index = selectable[-1]
            except Exception:
                pass
        elif self.view == "list":
            try:
                lv = self.query_one("#list", MessageList)
                n = len(
                    self.messages
                    if not (self.subagent_item and not self.subagent_detail_msg)
                    else self.subagent_item.subagent_messages
                )
                if n > 0:
                    lv.index = n - 1
            except Exception:
                pass
        elif self.view == "detail":
            try:
                dv = self.query_one("#detail", DetailView)
                n = len(dv._items)
                if n > 0:
                    dv.focus_item(n - 1)
            except Exception:
                pass

    def action_scroll_up(self) -> None:
        """u: scroll up in the current view."""
        match self.view:
            case "picker":
                with contextlib.suppress(Exception):
                    self.query_one("#picker", SourcePicker).scroll_up(amount=5, animate=False)
            case "list":
                with contextlib.suppress(Exception):
                    self.query_one("#list", MessageList).scroll_up(amount=5, animate=False)
            case "detail":
                with contextlib.suppress(Exception):
                    dv = self.query_one("#detail", DetailView)
                    dv.query_one("#items-scroll").scroll_up(amount=5, animate=False)

    def action_focus_sidebar(self) -> None:
        """h: focus the first focusable widget in the sidebar."""
        for widget in self._sidebar_widgets:
            for node in [widget, *widget.query("*")]:
                if node.focusable:
                    node.focus()
                    return

    def action_focus_content(self) -> None:
        """l: focus the active content widget."""
        self._focus_active_widget()

    def action_open_picker(self) -> None:
        """s: open session picker from list view; refocus picker if already there."""
        match self.view:
            case "picker":
                self.call_after_refresh(self._focus_active_widget)
            case "list":
                self.view = "picker"
            case _:
                pass

    def _focus_active_widget(self) -> None:
        """Move keyboard focus to the primary interactive widget for the current view."""
        with contextlib.suppress(Exception):
            match self.view:
                case "picker":
                    self.query_one("#picker", SourcePicker).focus()
                case "list":
                    self.query_one("#list", MessageList).focus()
                case "detail":
                    with contextlib.suppress(Exception):
                        self.query_one("#detail", DetailView).query_one("#items-list").focus()
                case view if view in self._view_owners:
                    self._view_owners[view].focus_view(view)

    # ----------------------------------------------------------------
    # Detail-view Enter handling
    # ----------------------------------------------------------------

    def _handle_detail_enter(self, idx: int) -> None:
        """Enter on an item in the detail items list."""
        msg = self._active_detail_message()
        if not msg:
            return
        items = msg.items or []
        if idx < 0 or idx >= len(items):
            return
        item = items[idx]

        if len(item.subagent_messages) > 0:
            # Drill into subagent
            if self.subagent_detail_msg:
                # Already in subagent detail — toggle expand instead of deeper drill
                self._toggle_expanded_item(idx)
            else:
                self.subagent_item = item
                self.subagent_detail_msg = None
                self._subagent_expanded_msgs = set()
                self._sync_subagent_message_list()
                self.view = "list"
        else:
            self._toggle_expanded_item(idx)

    # ----------------------------------------------------------------
    # Toggle helpers
    # ----------------------------------------------------------------

    def _toggle_expanded_message(self, idx: int) -> None:
        new_set = set(self.expanded_messages)
        if idx in new_set:
            new_set.discard(idx)
        else:
            new_set.add(idx)
        self.expanded_messages = new_set
        # Refresh just this item in the ListView
        try:
            ml = self.query_one("#list", MessageList)
            ml._expanded_set = self.expanded_messages
            ml.refresh_item(idx)
        except Exception:
            self._sync_message_list()

    def _toggle_expanded_item(self, idx: int) -> None:
        new_set = set(self.expanded_items)
        if idx in new_set:
            new_set.discard(idx)
        else:
            new_set.add(idx)
        self.expanded_items = new_set
        # Toggle the Collapsible directly
        try:
            dv = self.query_one("#detail", DetailView)
            dv.toggle_item(idx)
        except Exception:
            self._sync_detail_view()

    # ----------------------------------------------------------------
    # Index helpers
    # ----------------------------------------------------------------

    def _current_list_index(self) -> int | None:
        try:
            return self.query_one("#list", MessageList).index
        except Exception:
            return None

    def _current_detail_item_index(self) -> int | None:
        try:
            dv = self.query_one("#detail", DetailView)
            return dv.current_item_index()
        except Exception:
            return None

    def _active_detail_message(self) -> DisplayMessage | None:
        if self.subagent_detail_msg:
            return self.subagent_detail_msg
        # `self.messages` (from load_session) has tool bodies stripped for the
        # list view — the detail view shows the on-demand full-body fetch
        # instead (None while it's still loading; DetailView renders its own
        # loading state for that).
        return getattr(self, "_detail_full_message", None)

    # ----------------------------------------------------------------
    # Sync methods — push state into widgets
    # ----------------------------------------------------------------

    def _sync_all_widgets(self) -> None:
        self._sync_info_bar()
        for plugin in self.plugins.enabled():
            plugin.sync_sidebar()
        self._sync_content_view()

    def _sync_info_bar(self) -> None:
        if not self.session_path or self.view == "picker":
            self.sub_title = ""
            return
        parts: list[str] = []
        # last segment of path
        parts.append(self.session_path.split("/")[-1][:40])
        if self.meta.git_branch:
            parts.append(f"@{self.meta.git_branch}")
        if self.meta.permission_mode and self.meta.permission_mode != "default":
            parts.append(self.meta.permission_mode)
        if self.ongoing:
            parts.append("⬤ live")
        if self.totals and self.totals.cost_usd > 0:
            from format_utils import format_cost, format_tokens

            parts.append(format_tokens(self.totals.total_tokens))
            parts.append(format_cost(self.totals.cost_usd))
        self.sub_title = "  ·  ".join(parts)

    def _sync_session_picker(self) -> None:
        with contextlib.suppress(Exception):
            self.query_one("#picker", SourcePicker).populate(
                [(plugin.id, plugin.picker_section()) for plugin in self.plugins.enabled()]
            )

    def _sync_message_list(self) -> None:
        try:
            ml = self.query_one("#list", MessageList)
            # populate() is async (it awaits clear/append on ListView). Run
            # it in an exclusive worker so back-to-back populates serialize
            # — a new populate cancels any pending one and the next pass
            # will full-rebuild from a fresh node_count, so cancellation
            # leaves no inconsistent state.
            self.run_worker(
                ml.populate(
                    messages=self.messages,
                    expanded_set=self.expanded_messages,
                    ongoing=self.ongoing,
                ),
                exclusive=True,
                group="populate_msglist",
                name="populate_msglist",
            )
        except Exception:
            pass

    def _sync_subagent_message_list(self) -> None:
        """Show the subagent's messages in the list pane."""
        if not self.subagent_item:
            return
        try:
            ml = self.query_one("#list", MessageList)
            self.run_worker(
                ml.populate(
                    messages=self.subagent_item.subagent_messages,
                    expanded_set=self._subagent_expanded_msgs,
                    ongoing=self.subagent_item.subagent_ongoing,
                ),
                exclusive=True,
                group="populate_msglist",
                name="populate_msglist",
            )
        except Exception:
            pass

    def _sync_detail_view(self) -> None:
        """Rebuild DetailView for the active message."""
        try:
            dv = self.query_one("#detail", DetailView)
            msg = self._active_detail_message()
            depth = 1 if self.subagent_detail_msg else 0
            ongoing = False if self.subagent_detail_msg else self.ongoing
            dv.populate(
                message=msg,
                expanded_items=self.expanded_items,
                ongoing=ongoing,
                depth=depth,
            )
        except Exception:
            pass

    def _sync_detail_view_for_message_index(self, msg_idx: int) -> None:
        """Store which message index was opened and rebuild detail view.

        Clears any previously fetched full message first (so the view shows
        its loading state, not stale data from the last message), then
        fetches the new one on demand — `self.messages` only has lightened
        bodies.
        """
        self._detail_msg_idx = msg_idx
        self._detail_full_message = None
        self._sync_detail_view()
        if self.session_path:
            self.run_worker(
                self._fetch_detail_message(self.session_path, msg_idx),
                exclusive=True,
                group="load_detail_message",
                name="load_detail_message",
            )

    async def _fetch_detail_message(self, path: str, msg_idx: int) -> None:
        plugin = self._session_plugin
        try:
            msg = await plugin.load_full_message(path, msg_idx) if plugin else None
        except Exception:
            msg = None
        # Stale if the user moved on (closed detail, opened a different
        # message, or switched sessions) while this was in flight.
        if (
            self.session_path != path
            or getattr(self, "_detail_msg_idx", None) != msg_idx
            or self.view != "detail"
        ):
            return
        self._detail_full_message = msg
        self._sync_detail_view()

    def _sync_content_view(self) -> None:
        try:
            switcher = self.query_one("#content-switcher", ContentSwitcher)

            if self.view == "picker":
                switcher.current = "picker"
                self._sync_session_picker()

            elif self.view == "list":
                switcher.current = "list"
                if self.subagent_item and not self.subagent_detail_msg:
                    self._sync_subagent_message_list()
                else:
                    self._sync_message_list()

            elif self.view == "detail":
                switcher.current = "detail"
                self._sync_detail_view()

            elif self.view in self._view_owners:
                switcher.current = self.view
                self._view_owners[self.view].sync_view(self.view)

        except Exception:
            pass

    # ----------------------------------------------------------------
    # Reactive watchers → sync widgets
    # ----------------------------------------------------------------

    def watch_view(self, _v: str) -> None:
        self._sync_all_widgets()
        self.call_after_refresh(self._focus_active_widget)

    def watch_messages(self, _m) -> None:
        if self.view == "list":
            self._sync_message_list()

    def watch_expanded_messages(self, _e) -> None:
        if self.view == "list":
            self._sync_message_list()

    def watch_expanded_items(self, _e) -> None:
        if self.view == "detail":
            self._sync_detail_view()

    def watch_subagent_item(self, _i) -> None:
        if self.view in ("list", "detail"):
            self._sync_content_view()

    def watch_subagent_detail_msg(self, _m) -> None:
        if self.view == "detail":
            self._sync_detail_view()

    def watch_ongoing(self, _o) -> None:
        self._sync_info_bar()
        if self.view in ("list", "detail"):
            self._sync_content_view()

    def watch_meta(self, _m) -> None:
        self._sync_info_bar()

    def watch_totals(self, _t) -> None:
        self._sync_info_bar()

    def watch_session_path(self, _p) -> None:
        self._sync_info_bar()
