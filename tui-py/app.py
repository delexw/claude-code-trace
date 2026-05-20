"""Main Textual application for Claude Code Trace TUI.

Architecture:
- App owns all state.
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
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.reactive import reactive
from textual.widget import Widget
from textual.widgets import ContentSwitcher, Footer, Header, ListView

import api as api_client
from data_types import (
    DisplayMessage,
    SessionInfo,
    SessionMeta,
    SessionTotals,
    session_update_from_dict,
)
from format_utils import project_key as _project_key
from sse import SSEClient
from widgets.debug_viewer import DebugViewer
from widgets.detail_view import DetailView
from widgets.message_list import MessageList
from widgets.project_tree import ProjectTree
from widgets.session_picker import SessionPicker
from widgets.team_board import TeamBoard

API_BASE = "http://127.0.0.1:11423"


class SidebarResizer(Widget):
    """1-column drag handle between the ProjectTree sidebar and the content pane."""

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
                self.app.query_one("#project-tree", ProjectTree).styles.width = new_width

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
    ]

    # ---- State ----
    view: reactive[str] = reactive("picker")

    # Picker
    all_sessions: reactive[list[SessionInfo]] = reactive(list)
    picker_loading: reactive[bool] = reactive(True)
    picker_error: reactive[str] = reactive("")
    selected_project: reactive[str | None] = reactive(None)

    # Session
    session_path: reactive[str] = reactive("")
    messages: reactive[list] = reactive(list)
    teams: reactive[list] = reactive(list)
    ongoing: reactive[bool] = reactive(False)
    meta: reactive[SessionMeta] = reactive(SessionMeta)
    totals: reactive[SessionTotals] = reactive(SessionTotals)
    debug_entries: reactive[list] = reactive(list)

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
        self._project_dirs: list[str] = []
        self._sse: SSEClient | None = None
        self._subagent_expanded_msgs: set[int] = set()
        self._subagent_detail_item_idx: int = 0

    # ----------------------------------------------------------------
    # Layout
    # ----------------------------------------------------------------

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="main-area"):
            yield ProjectTree(id="project-tree")
            yield SidebarResizer(id="sidebar-resizer")
            with ContentSwitcher(initial="picker", id="content-switcher"):
                yield SessionPicker(id="picker")
                yield MessageList(id="list")
                yield DetailView(id="detail")
                yield TeamBoard(id="team")
                yield DebugViewer(id="debug")
        yield Footer()

    # ----------------------------------------------------------------
    # Lifecycle
    # ----------------------------------------------------------------

    def on_mount(self) -> None:
        self.title = "cctrace"
        self.run_worker(self._init_sessions(), exclusive=False, name="init")
        self._start_sse()

    def _start_sse(self) -> None:
        self._sse = SSEClient(f"{API_BASE}/api/events")
        self._sse.on("picker-refresh", self._on_picker_refresh)
        self._sse.on("session-update", self._on_session_update)
        self._sse.start()

    async def on_unmount(self) -> None:
        if self._sse:
            self._sse.stop()
        # Best-effort cleanup with a short timeout; don't block exit.
        with contextlib.suppress(Exception):
            await asyncio.wait_for(self._cleanup(), timeout=1.0)

    async def _cleanup(self) -> None:
        await api_client.unwatch_session()
        await api_client.unwatch_picker()

    # ----------------------------------------------------------------
    # Session initialisation
    # ----------------------------------------------------------------

    async def _init_sessions(self) -> None:
        for attempt in range(11):
            try:
                dirs = await api_client.get_project_dirs()
                if not dirs:
                    self.picker_error = (
                        "No project directories found. Run the desktop app first to configure."
                    )
                    self.picker_loading = False
                    return
                sessions = await api_client.discover_sessions(dirs)
                self._project_dirs = dirs
                self.all_sessions = sessions
                self.picker_loading = False
                await api_client.watch_picker(dirs)
                return
            except Exception as e:
                if attempt < 10:
                    await asyncio.sleep(1)
                else:
                    self.picker_error = f"Cannot connect to backend. Is the app running?\n{e}"
                    self.picker_loading = False

    # ----------------------------------------------------------------
    # SSE handlers
    # ----------------------------------------------------------------

    async def _on_picker_refresh(self, _payload) -> None:
        try:
            dirs = await api_client.get_project_dirs()
            if dirs:
                self._project_dirs = dirs
        except Exception:
            dirs = self._project_dirs
        if dirs:
            try:
                sessions = await api_client.discover_sessions(dirs)
                self.all_sessions = sessions
            except Exception:
                pass

    async def _on_session_update(self, payload) -> None:
        if not isinstance(payload, dict):
            return
        messages, ongoing, permission_mode, teams, totals = session_update_from_dict(payload)
        self.messages = messages
        self.ongoing = ongoing
        self.totals = totals
        if teams:
            self.teams = teams
        if permission_mode:
            self.meta = SessionMeta(
                cwd=self.meta.cwd,
                git_branch=self.meta.git_branch,
                permission_mode=permission_mode,
            )
        self._sync_all_widgets()

    # ----------------------------------------------------------------
    # Session loading
    # ----------------------------------------------------------------

    async def _load_session(self, path: str) -> None:
        """Fetch session data, fully populate the MessageList, then switch
        view. Keeps the LoadingIndicator on the *picker* until the list is
        truly ready, so the user never lands on a half-built list pane
        where j/k briefly does nothing.
        """
        picker = None
        with contextlib.suppress(Exception):
            picker = self.query_one("#picker", SessionPicker)
            picker.loading = True
        try:
            result = await api_client.load_session(path)
            self.session_path = path
            # Set everything except `messages` first — `messages` triggers
            # watch_messages, but that watcher early-returns while view !=
            # "list", so the SyncMessageList worker doesn't race us.
            self.teams = result.teams
            self.ongoing = result.ongoing
            self.meta = result.meta
            self.totals = result.session_totals
            self.expanded_messages = set()
            self.messages = result.messages
            # Pre-populate the MessageList synchronously (awaited) so it's
            # fully built before we flip the view.
            with contextlib.suppress(Exception):
                ml = self.query_one("#list", MessageList)
                await ml.populate(
                    messages=result.messages,
                    expanded_set=self.expanded_messages,
                    ongoing=result.ongoing,
                )
            await api_client.watch_session(path)
            self.view = "list"
        except Exception:
            pass
        finally:
            if picker is not None:
                with contextlib.suppress(Exception):
                    picker.loading = False

    # ----------------------------------------------------------------
    # Widget event handlers — ListView, Tree
    # ----------------------------------------------------------------

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle Enter on a ListView item."""
        lv_id = event.list_view.id

        if lv_id == "picker":
            # event.index is the raw ListView node index (including disabled header rows)
            picker = self.query_one("#picker", SessionPicker)
            session = picker.session_at_raw_index(event.index)
            if session is not None:
                # _load_session shows a LoadingIndicator on the picker,
                # fully populates the MessageList, *then* flips the view —
                # so the user never lands on a half-built list pane.
                self._detail_item_idx = 0
                self.expanded_items = set()
                self.subagent_item = None
                self.subagent_detail_msg = None
                self.run_worker(
                    self._load_session(session.path),
                    exclusive=True,
                    group="load_session",
                    name="load_session",
                )

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

    def on_project_tree_project_selected(self, event: ProjectTree.ProjectSelected) -> None:
        """Handle project selection from the sidebar Tree."""
        if event.project_key != self.selected_project:
            # Different project chosen — update filter and switch to picker view.
            self.selected_project = event.project_key
            self.view = "picker"
        else:
            # Same project (e.g. l/right/Esc to dismiss sidebar) — just refocus content.
            self._focus_active_widget()

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
                    self.view = "list"
            case "team" | "debug":
                self.view = "list"

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
        """r: re-discover sessions and update the project tree."""
        self.picker_loading = True
        self.picker_error = ""
        self.run_worker(self._init_sessions(), exclusive=True, name="refresh")

    def action_show_teams(self) -> None:
        if self.view == "list" and self.teams:
            self.view = "team"

    def action_d_action(self) -> None:
        """d: scroll down in current view, or show debug log from list view."""
        match self.view:
            case "picker":
                with contextlib.suppress(Exception):
                    self.query_one("#picker", SessionPicker).scroll_down(amount=5, animate=False)
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
                lv = self.query_one("#picker", SessionPicker)
                if lv._index_to_session:
                    first_raw = min(lv._index_to_session.keys())
                    lv.index = first_raw
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
                lv = self.query_one("#picker", SessionPicker)
                if lv._index_to_session:
                    last_raw = max(lv._index_to_session.keys())
                    lv.index = last_raw
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
                    self.query_one("#picker", SessionPicker).scroll_up(amount=5, animate=False)
            case "list":
                with contextlib.suppress(Exception):
                    self.query_one("#list", MessageList).scroll_up(amount=5, animate=False)
            case "detail":
                with contextlib.suppress(Exception):
                    dv = self.query_one("#detail", DetailView)
                    dv.query_one("#items-scroll").scroll_up(amount=5, animate=False)

    def action_focus_sidebar(self) -> None:
        """h: focus the project tree sidebar."""
        with contextlib.suppress(Exception):
            self.query_one("#tree-inner").focus()

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
                    self.query_one("#picker", SessionPicker).focus()
                case "list":
                    self.query_one("#list", MessageList).focus()
                case "detail":
                    with contextlib.suppress(Exception):
                        self.query_one("#detail", DetailView).query_one("#items-list").focus()
                case "debug":
                    self.query_one("#debug", DebugViewer).focus()

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
        if self.messages:
            idx = getattr(self, "_detail_msg_idx", 0)
            if 0 <= idx < len(self.messages):
                return self.messages[idx]
        return None

    # ----------------------------------------------------------------
    # Session filter
    # ----------------------------------------------------------------

    def _picker_sessions(self) -> list[SessionInfo]:
        if self.selected_project is None:
            return self.all_sessions
        key = self.selected_project
        return [s for s in self.all_sessions if _project_key(s.path) == key]

    # ----------------------------------------------------------------
    # Sync methods — push state into widgets
    # ----------------------------------------------------------------

    def _sync_all_widgets(self) -> None:
        self._sync_info_bar()
        self._sync_project_tree()
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

    def _sync_project_tree(self) -> None:
        try:
            tree = self.query_one("#project-tree", ProjectTree)
            collapsed: set[str] = set()  # collapse state tracked in Tree widget itself
            tree.update_state(
                sessions=self.all_sessions,
                selected_project=self.selected_project,
                collapsed_keys=collapsed,
            )
        except Exception:
            pass

    def _sync_session_picker(self) -> None:
        try:
            picker = self.query_one("#picker", SessionPicker)
            picker.populate(
                sessions=self._picker_sessions(),
                loading=self.picker_loading,
                error=self.picker_error,
            )
        except Exception:
            pass

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
        """Store which message index was opened and rebuild detail view."""
        self._detail_msg_idx = msg_idx
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

            elif self.view == "team":
                switcher.current = "team"
                tb = self.query_one("#team", TeamBoard)
                tb.teams = self.teams

            elif self.view == "debug":
                switcher.current = "debug"
                dv = self.query_one("#debug", DebugViewer)
                dv.populate(self.debug_entries)

        except Exception:
            pass

    # ----------------------------------------------------------------
    # Async helpers
    # ----------------------------------------------------------------

    async def _fetch_debug_log(self) -> None:
        if self.session_path:
            try:
                entries = await api_client.get_debug_log(self.session_path)
                self.debug_entries = entries
                self._sync_content_view()
            except Exception:
                pass

    # ----------------------------------------------------------------
    # Reactive watchers → sync widgets
    # ----------------------------------------------------------------

    def watch_view(self, _v: str) -> None:
        self._sync_all_widgets()
        self.call_after_refresh(self._focus_active_widget)

    def watch_all_sessions(self, _s) -> None:
        self._sync_project_tree()
        if self.view == "picker":
            self._sync_session_picker()

    def watch_picker_loading(self, _l) -> None:
        if self.view == "picker":
            self._sync_session_picker()

    def watch_picker_error(self, _e) -> None:
        if self.view == "picker":
            self._sync_session_picker()

    def watch_selected_project(self, _p) -> None:
        if self.view == "picker":
            self._sync_session_picker()

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

    def watch_teams(self, _t) -> None:
        if self.view == "team":
            self._sync_content_view()

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
