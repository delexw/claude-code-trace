"""The Claude Code session source."""

from __future__ import annotations

import asyncio
import contextlib
from collections.abc import Iterable, Mapping

import api as api_client
from data_types import (
    DisplayMessage,
    IndexProgress,
    LoadResult,
    SessionInfo,
    index_progress_from_dict,
)
from efficiency import (
    EfficiencyJob,
    EfficiencySummary,
    job_from_dict,
    latest_jobs_by_session,
)
from format_utils import project_key as _project_key
from plugins.base import (
    LIST_VIEW,
    PICKER_VIEW,
    OpenedSession,
    PickerSection,
    PluginKey,
    SourcePlugin,
    SseHandler,
)
from plugins.claude.picker_rows import build_section
from plugins.claude.sidebar import ClaudeProjectTree
from project_tree import resolve_fork_root
from resume_command import resume_command
from widgets.analytics_settings import AnalyticsSettingsScreen
from widgets.debug_viewer import DebugViewer
from widgets.efficiency_privacy import EfficiencyPrivacyScreen
from widgets.index_progress import IndexProgressBar
from widgets.team_board import TeamBoard

# A failed analysis is worth reading; the default 5s toast is not enough.
FAILURE_NOTICE_SECONDS = 30

JEV_KEY_REQUIRED = (
    "Set JEV_API_KEY before starting the backend, or store the key with the desktop "
    "app, then reopen the analytics settings to confirm it."
)

TEAM_VIEW = "team"
DEBUG_VIEW = "debug"


class ClaudePlugin(SourcePlugin):
    """Sessions from Claude Code's JSONL transcripts."""

    id = "claude"
    label = "Claude"
    can_disable = False
    KEYS = (
        # Per-view: only relevant (and shown) once a session is in context —
        # see check_action. Deliberately NOT shown in the picker footer, which
        # already overflows on narrow terminals.
        PluginKey("y", "copy_resume", "Copy resume"),
        # Picker-only, for the same reason — see check_action.
        PluginKey("a", "analyse_session", "Analyse"),
        PluginKey("comma", "analytics_settings", "Analytics"),
    )

    def __init__(self, host) -> None:
        super().__init__(host)
        self.all_sessions: list[SessionInfo] = []
        self.picker_loading: bool = True
        self.picker_error: str = ""
        self.selected_project: str | None = None
        # How far the backend's walk of the project directories has got.
        self.index_progress: IndexProgress | None = None
        # Jev efficiency analysis, keyed by session path. Includes analyses
        # started from the desktop app or a browser against the same backend.
        self.efficiency_jobs: dict[str, EfficiencyJob] = {}
        self.efficiency_summaries: dict[str, EfficiencySummary] = {}
        # The SessionInfo backing the currently open session (list/detail
        # views) — needed for copy_resume (cwd + session_id). None in the
        # picker, where no session is selected yet.
        self.current_session: SessionInfo | None = None
        self.teams: list = []
        self.debug_entries: list = []
        self._project_dirs: list[str] = []
        self._tree: ClaudeProjectTree | None = None
        self._progress_bar: IndexProgressBar | None = None
        self._team_board: TeamBoard | None = None
        self._debug_viewer: DebugViewer | None = None

    # ----------------------------------------------------------------
    # Lifecycle
    # ----------------------------------------------------------------

    def start(self) -> None:
        self.host.run_worker(self.init_sessions(), exclusive=False, name="init")
        self.host.run_worker(self.load_efficiency_state(), exclusive=False, name="efficiency")

    async def stop(self) -> None:
        await api_client.unwatch_session()
        await api_client.unwatch_picker()

    def refresh(self) -> None:
        """r: re-discover sessions and update the project tree."""
        self._set_picker_state(loading=True, error="")
        self.host.run_worker(self.init_sessions(), exclusive=True, name="refresh")

    def sse_handlers(self) -> Mapping[str, SseHandler]:
        return {
            "picker-refresh": self.on_picker_refresh,
            "session-update": self.on_session_update,
            "index-progress": self.on_index_progress,
            "efficiency-analysis-update": self.on_efficiency_update,
        }

    # ----------------------------------------------------------------
    # Session discovery
    # ----------------------------------------------------------------

    async def init_sessions(self) -> None:
        for attempt in range(11):
            try:
                dirs = await api_client.get_project_dirs()
                if not dirs:
                    self._set_picker_state(
                        loading=False,
                        error="No project directories found. Run the desktop app first to configure.",
                    )
                    return
                sessions = await api_client.discover_sessions(dirs)
                self._project_dirs = dirs
                self._set_sessions(sessions)
                self._set_picker_state(loading=False, error=self.picker_error)
                await api_client.watch_picker(dirs)
                return
            except Exception as e:
                if attempt < 10:
                    await asyncio.sleep(1)
                else:
                    self._set_picker_state(
                        loading=False,
                        error=f"Cannot connect to backend. Is the app running?\n{e}",
                    )

    # Each setter re-syncs only on an actual change, so an unchanged refresh
    # does not rebuild the picker and drop the cursor.

    def _set_sessions(self, sessions: list[SessionInfo]) -> None:
        if sessions == self.all_sessions:
            return
        self.all_sessions = sessions
        self.sync_sidebar()
        self._sync_picker_if_shown()

    def _set_picker_state(self, loading: bool, error: str) -> None:
        if (loading, error) == (self.picker_loading, self.picker_error):
            return
        self.picker_loading = loading
        self.picker_error = error
        self._sync_picker_if_shown()

    def _sync_picker_if_shown(self) -> None:
        if self.host.view == PICKER_VIEW:
            self.host.sync_picker()

    # ----------------------------------------------------------------
    # SSE handlers
    # ----------------------------------------------------------------

    async def on_picker_refresh(self, _payload) -> None:
        try:
            dirs = await api_client.get_project_dirs()
            if dirs:
                self._project_dirs = dirs
        except Exception:
            dirs = self._project_dirs
        if dirs:
            try:
                sessions = await api_client.discover_sessions(dirs)
                self._set_sessions(sessions)
            except Exception:
                pass

    async def on_session_update(self, _payload) -> None:
        """`session-update` is a lightweight refresh signal (message count +
        roles only, no message bodies) — the client re-fetches the session on
        receipt. Re-fetching here (instead of trusting the signal payload)
        keeps the shown messages authoritative; the backend's light cache
        makes repeated fetches during an active stream cheap.
        """
        path = self.host.session_path
        if not path:
            return
        try:
            result = await api_client.load_session(path)
        except Exception:
            return
        self._set_teams(result.teams)
        self.host.apply_session_update(_opened(path, result))

    def on_index_progress(self, payload) -> None:
        """`index-progress`: how much of the projects directory has been read.

        Sent while the backend walks the session files — the picker fills in
        newest-first as it goes, so this says how much is still to come.
        """
        self.index_progress = index_progress_from_dict(payload or {})
        if self._progress_bar is not None:
            self._progress_bar.progress = self.index_progress

    async def on_efficiency_update(self, payload) -> None:
        """`efficiency-analysis-update`: one job's progress, from any client."""
        job = job_from_dict(payload or {})
        if not job.session_path:
            return
        self._set_jobs({**self.efficiency_jobs, job.session_path: job})
        if job.status == "completed":
            # The score lands in the summaries, not in the job.
            await self._refresh_efficiency_summaries()
        elif job.status == "failed":
            # Raised as well as written into the row: an analysis takes minutes,
            # by which time the user is usually looking at something else.
            name = job.session_name or job.session_id or job.session_path
            self.host.notify(
                f"{name}: {job.error or job.message}",
                title="Jev analysis failed",
                severity="error",
                timeout=FAILURE_NOTICE_SECONDS,
                markup=False,
            )

    # ----------------------------------------------------------------
    # Jev efficiency analysis
    # ----------------------------------------------------------------

    async def load_efficiency_state(self) -> None:
        """Pick up analyses already run against this backend — from the
        desktop app, a browser, or an earlier TUI run — so the picker shows
        their scores and any analysis still in flight."""
        try:
            jobs = await api_client.list_efficiency_jobs()
            summaries = await api_client.list_efficiency_summaries()
        except Exception:
            return
        self._set_jobs(latest_jobs_by_session(jobs))
        self._set_summaries({s.session_path: s for s in summaries})

    async def _refresh_efficiency_summaries(self) -> None:
        try:
            summaries = await api_client.list_efficiency_summaries()
        except Exception:
            return
        self._set_summaries({s.session_path: s for s in summaries})

    def _set_jobs(self, jobs: dict[str, EfficiencyJob]) -> None:
        if jobs == self.efficiency_jobs:
            return
        self.efficiency_jobs = jobs
        self._sync_picker_analysis()

    def _set_summaries(self, summaries: dict[str, EfficiencySummary]) -> None:
        if summaries == self.efficiency_summaries:
            return
        self.efficiency_summaries = summaries
        self._sync_picker_analysis()

    def _sync_picker_analysis(self) -> None:
        """Re-render only the analysed rows. A full rebuild would clear the
        list and drop the cursor, and a running job reports progress often."""
        analysed = [
            s
            for s in self.all_sessions
            if s.path in self.efficiency_jobs or s.path in self.efficiency_summaries
        ]
        self.host.refresh_picker_rows(self.id, analysed)

    async def analyse_session(self, session: SessionInfo) -> None:
        """Prepare, confirm, then send one session to Jev.

        The payload is built and redacted locally first; nothing leaves the
        machine until the privacy notice is accepted, which is asked again
        for every analysis and re-analysis.
        """
        try:
            settings = await api_client.get_analytics_settings()
        except Exception as error:
            self.host.notify(
                str(error), title="Analytics settings unavailable", severity="error", markup=False
            )
            return
        if not settings.jev.configured:
            self.host.notify(
                JEV_KEY_REQUIRED, title="Jev API key required", severity="warning", markup=False
            )
            return
        try:
            payload = await api_client.prepare_efficiency_payload(
                session.path, settings.default_payload_mode
            )
        except Exception as error:
            self.host.notify(
                str(error), title="Could not prepare the analysis", severity="error", markup=False
            )
            return

        if not await self.host.push_screen_wait(EfficiencyPrivacyScreen(payload)):
            return

        try:
            job = await api_client.start_efficiency_analysis(payload)
        except Exception as error:
            self.host.notify(
                str(error), title="Could not start the analysis", severity="error", markup=False
            )
            return
        self._set_jobs({**self.efficiency_jobs, job.session_path: job})

    async def _open_analytics_settings(self) -> None:
        try:
            settings = await api_client.get_analytics_settings()
        except Exception as error:
            self.host.notify(
                str(error), title="Analytics settings unavailable", severity="error", markup=False
            )
            return
        self.host.push_screen(AnalyticsSettingsScreen(settings))

    # ----------------------------------------------------------------
    # Picker
    # ----------------------------------------------------------------

    def picker_section(self) -> PickerSection:
        return build_section(
            self._picker_sessions(),
            loading=self.picker_loading,
            error=self.picker_error,
            analysis=lambda path: (
                self.efficiency_jobs.get(path),
                self.efficiency_summaries.get(path),
            ),
        )

    def _picker_sessions(self) -> list[SessionInfo]:
        if self.selected_project is None:
            return self.all_sessions
        key = self.selected_project
        # Resolve each session's fork chain against the full list so a forked session —
        # grouped under its fork parent's project in the sidebar tree, see
        # project_tree.build_project_nodes — is included too (#238).
        sessions_by_id = {s.session_id: s for s in self.all_sessions}
        return [
            s
            for s in self.all_sessions
            if _project_key(resolve_fork_root(s, sessions_by_id).path) == key
        ]

    def open(self, item: object) -> None:
        if not isinstance(item, SessionInfo):
            return
        self.current_session = item
        self.host.run_worker(
            self._load_session(item.path),
            exclusive=True,
            group="load_session",
            name="load_session",
        )

    async def _load_session(self, path: str) -> None:
        """Fetch session data, fully populate the MessageList, then switch
        view. Keeps the LoadingIndicator on the *picker* until the list is
        truly ready, so the user never lands on a half-built list pane
        where j/k briefly does nothing.
        """
        self.host.set_picker_busy(True)
        try:
            result = await api_client.load_session(path)
            self._set_teams(result.teams)
            await self.host.present_session(self, _opened(path, result))
            await api_client.watch_session(path)
            self.host.view = LIST_VIEW
        except Exception:
            pass
        finally:
            self.host.set_picker_busy(False)

    async def load_full_message(self, path: str, index: int) -> DisplayMessage | None:
        return await api_client.load_message(path, index)

    # ----------------------------------------------------------------
    # Sidebar, status and own views
    # ----------------------------------------------------------------

    def compose_sidebar(self) -> Iterable:
        self._tree = ClaudeProjectTree(self._select_project, id="project-tree")
        yield self._tree

    def compose_status(self) -> Iterable:
        self._progress_bar = IndexProgressBar(id="index-progress")
        yield self._progress_bar

    def compose_views(self) -> Iterable:
        self._team_board = TeamBoard(id=TEAM_VIEW)
        self._debug_viewer = DebugViewer(id=DEBUG_VIEW)
        yield self._team_board
        yield self._debug_viewer

    def sync_view(self, view: str) -> None:
        if view == TEAM_VIEW and self._team_board is not None:
            self._team_board.teams = self.teams
        elif view == DEBUG_VIEW and self._debug_viewer is not None:
            self._debug_viewer.populate(self.debug_entries)

    def focus_view(self, view: str) -> None:
        if view == DEBUG_VIEW and self._debug_viewer is not None:
            self._debug_viewer.focus()

    def _select_project(self, project_key: str | None) -> None:
        if project_key != self.selected_project:
            # Different project chosen — update the filter and show the picker.
            self.selected_project = project_key
            self._sync_picker_if_shown()
            self.host.view = PICKER_VIEW
        else:
            # Same project (e.g. l/right/Esc to dismiss sidebar) — just refocus content.
            self.host.focus_content()

    def sync_sidebar(self) -> None:
        if self._tree is None:
            return
        with contextlib.suppress(Exception):
            # Collapse state is tracked in the Tree widget itself.
            self._tree.update_state(
                sessions=self.all_sessions,
                selected_project=self.selected_project,
                collapsed_keys=set(),
            )

    def _set_teams(self, teams: list) -> None:
        if teams == self.teams:
            return
        self.teams = teams
        if self.host.view == TEAM_VIEW:
            self.host.sync_view()

    async def _fetch_debug_log(self) -> None:
        path = self.host.session_path
        if path:
            try:
                self.debug_entries = await api_client.get_debug_log(path)
                self.host.sync_view()
            except Exception:
                pass

    # ----------------------------------------------------------------
    # Keys
    # ----------------------------------------------------------------

    def check_action(self, action: str, view: str) -> bool | None:
        """copy_resume is hidden outside the list/detail views (no session in
        context in the picker), and the Jev keys are shown only in the
        picker — both kept off the global footer, which already overflows on
        narrow terminals."""
        if action == "copy_resume":
            return view in ("list", "detail") and self.current_session is not None
        if action in ("analyse_session", "analytics_settings"):
            return view == PICKER_VIEW
        return True

    def action_copy_resume(self) -> None:
        """y: copy the `claude --resume` command for the current session."""
        sel = self.current_session
        if sel is None:
            return
        self.host.copy_to_clipboard(resume_command(sel.cwd, sel.session_id))

    def action_analyse_session(self) -> None:
        """a: analyse the highlighted session's efficiency with Jev."""
        session = self.host.highlighted_picker_item(self.id)
        if not isinstance(session, SessionInfo):
            return
        self.host.run_worker(self.analyse_session(session), exclusive=True, group="analyse")

    def action_analytics_settings(self) -> None:
        """,: open the Jev and recommendation-provider settings."""
        self.host.run_worker(
            self._open_analytics_settings(), exclusive=True, group="analytics-settings"
        )

    def action_show_teams(self) -> None:
        if self.host.view == LIST_VIEW and self.teams:
            self.host.view = TEAM_VIEW


def _opened(path: str, result: LoadResult) -> OpenedSession:
    return OpenedSession(
        path=path,
        messages=result.messages,
        ongoing=result.ongoing,
        meta=result.meta,
        totals=result.session_totals,
    )
