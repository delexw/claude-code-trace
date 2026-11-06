"""Read-only Codex chat source for the terminal UI."""

from __future__ import annotations

import time

import api as api_client
from data_types import DisplayMessage, SessionMeta, SessionTotals, message_from_dict
from plugins.base import (
    DETAIL_VIEW,
    LIST_VIEW,
    PICKER_VIEW,
    OpenedSession,
    PickerSection,
    PluginKey,
)
from plugins.codex.models import CodexSession, session_from_dict
from plugins.codex.picker_rows import build_section
from plugins.codex.projects import build_project_items, filter_project_sessions
from plugins.codex.resume_command import resume_command
from plugins.codex.sidebar import CodexProjectTree
from plugins.efficiency import EfficiencySourcePlugin


async def load_conversation(session: CodexSession) -> OpenedSession:
    data = await api_client.post_json("/api/codex/session", {"path": session.path})
    return _opened_session(session, data)


def _label_messages(messages: list[DisplayMessage]) -> None:
    for message in messages:
        message.role_label = "Codex" if message.role == "claude" else message.role.title()
        for item in message.items:
            _label_messages(item.subagent_messages)


def _opened_session(
    session: CodexSession, data: object, messages: list[DisplayMessage] | None = None
) -> OpenedSession:
    if not isinstance(data, dict):
        raise ValueError("Invalid Codex session response")
    if messages is None:
        messages = [
            message_from_dict(raw) for raw in data.get("messages", []) if isinstance(raw, dict)
        ]
        _label_messages(messages)
    return OpenedSession(
        path=session.path,
        messages=messages,
        ongoing=bool(data.get("ongoing", False)),
        meta=SessionMeta(
            cwd=session.cwd,
            session_id=session.id,
            git_branch=str(data.get("git_branch", session.git_branch)),
            permission_mode=str(data.get("permission_mode", session.permission_mode)),
            sandbox_policy=str(data.get("sandbox_policy", session.sandbox_policy)),
            duration_ms=data.get("duration_ms", session.duration_ms),
            context_tokens=data.get("context_tokens", session.context_tokens),
            context_window=data.get("context_window", session.context_window) or 0,
        ),
        totals=SessionTotals(
            input_tokens=int(data.get("input_tokens", 0)),
            output_tokens=int(data.get("output_tokens", 0)),
            cache_read_tokens=int(data.get("cache_read_tokens", 0)),
            total_tokens=int(data.get("total_tokens", 0)),
            cost_usd=float(data["cost_usd"]) if data.get("cost_usd") is not None else None,
            estimated_cost=True,
            cost_partial=bool(data.get("cost_partial", session.cost_partial)),
            model=str(data.get("model") or session.model),
        ),
    )


class CodexPlugin(EfficiencySourcePlugin):
    id = "codex"
    label = "Codex"
    efficiency_prepare_endpoint = "/api/codex/efficiency/prepare"
    efficiency_source_label = "Codex"
    KEYS = (
        PluginKey("y", "copy_resume", "Copy resume"),
        PluginKey("a", "analyse_session", "Analyse"),
        PluginKey("comma", "analytics_settings", "Analytics"),
    )

    def __init__(self, host) -> None:
        super().__init__(host)
        self.sessions: list[CodexSession] = []
        self.loading = True
        self.error = ""
        self._session: CodexSession | None = None
        self._messages: list[DisplayMessage] = []
        self.selected_project: str | None = None
        self._tree: CodexProjectTree | None = None
        self._started = False
        self._polling = False
        self._last_picker_refresh = 0.0
        self._revision = ""
        self._opened: OpenedSession | None = None

    def start(self) -> None:
        self._started = True
        self.host.run_worker(self._fetch_sessions(), exclusive=True, group="codex-picker")
        self.host.run_worker(self.load_efficiency_state(), exclusive=True, group="codex-efficiency")

    async def stop(self) -> None:
        self._started = False

    def sse_handlers(self):
        return {"efficiency-analysis-update": self.on_efficiency_update}

    def efficiency_sessions(self):
        return self.sessions

    def refresh(self) -> None:
        self.start()
        self.host.run_worker(self._refresh_open(), exclusive=True, group="codex-refresh")

    async def _fetch_sessions(self) -> None:
        try:
            data = await api_client.get_json("/api/codex/sessions")
            self.sessions = (
                [session_from_dict(raw) for raw in data if isinstance(raw, dict)]
                if isinstance(data, list)
                else []
            )
            self.error = ""
        except Exception as exc:
            self.error = f"Could not list Codex conversations: {exc}"
        self.loading = False
        self._last_picker_refresh = time.monotonic()
        self.sync_sidebar()
        if self.host.view == PICKER_VIEW:
            self.host.sync_picker()

    def picker_section(self) -> PickerSection:
        return build_section(
            filter_project_sessions(self.sessions, self.selected_project),
            loading=self.loading,
            error=self.error,
            analysis=lambda path: (
                self.efficiency_jobs.get(path),
                self.efficiency_summaries.get(path),
            ),
        )

    def compose_sidebar(self):
        self._tree = CodexProjectTree(
            self._select_project, self._poll_once, id="codex-project-tree"
        )
        yield self._tree

    def sync_sidebar(self) -> None:
        if self._tree is not None:
            self._tree.update_items(build_project_items(self.sessions), self.selected_project)

    def _select_project(self, key: str | None) -> None:
        if key != self.selected_project:
            self.selected_project = key
            self.host.view = PICKER_VIEW
            self.host.sync_picker()
            self.sync_sidebar()
        self.host.focus_content()

    def check_action(self, action: str, view: str) -> bool | None:
        if action == "copy_resume":
            return (
                view in (LIST_VIEW, DETAIL_VIEW)
                and self._session is not None
                and self.host.session_path == self._session.path
            )
        if action in ("analyse_session", "analytics_settings"):
            return view == PICKER_VIEW and (
                action != "analyse_session"
                or isinstance(self.host.highlighted_picker_item(self.id), CodexSession)
            )
        return True

    def action_copy_resume(self) -> None:
        if self._session is not None and self.host.session_path == self._session.path:
            self.host.copy_to_clipboard(resume_command(self._session))

    def open(self, item: object) -> None:
        if isinstance(item, CodexSession):
            self.host.run_worker(
                self._load(item), exclusive=True, group="load_session", name="load_session"
            )

    async def _load(self, session: CodexSession) -> None:
        self.host.set_picker_busy(True)
        try:
            data = await api_client.post_json("/api/codex/session", {"path": session.path})
            opened = _opened_session(session, data)
            self._session, self._messages = session, opened.messages
            self._opened = opened
            self._revision = str(data.get("revision", ""))
            await self.host.present_session(self, opened)
            self.host.view = LIST_VIEW
        except Exception as exc:
            self.host.notify(f"Could not open Codex conversation: {exc}", severity="error")
        finally:
            self.host.set_picker_busy(False)

    async def _poll_once(self) -> None:
        if (
            not self._started
            or self._polling
            or (self._tree is not None and not self._tree.display)
        ):
            return
        self._polling = True
        try:
            if self.host.view == PICKER_VIEW:
                if time.monotonic() - self._last_picker_refresh >= 10:
                    await self._fetch_sessions()
                return
            session = self._session
            if (
                session is None
                or self.host.view not in (LIST_VIEW, DETAIL_VIEW)
                or self.host.session_path != session.path
            ):
                return
            data = await api_client.post_json(
                "/api/codex/session", {"path": session.path, "limit": 0}
            )
            if not isinstance(data, dict):
                return
            revision = str(data.get("revision", ""))
            messages = self._messages
            if revision != self._revision:
                # A revision can change an earlier message or a linked worker.
                # Unchanged sessions only make the metadata request above.
                updated = await api_client.post_json("/api/codex/session", {"path": session.path})
                if not isinstance(updated, dict):
                    return
                data = updated
                messages = [
                    message_from_dict(raw)
                    for raw in data.get("messages", [])
                    if isinstance(raw, dict)
                ]
                _label_messages(messages)
            if (
                self._started
                and (self._tree is None or self._tree.display)
                and self._session is session
                and self.host.session_path == session.path
            ):
                opened = _opened_session(session, data, messages)
                self._messages = messages
                self._revision = str(data.get("revision", revision))
                if opened != self._opened:
                    self._opened = opened
                    self.host.apply_session_update(opened)
        except Exception:
            # A transient live read must not interrupt browsing. Manual refresh reports errors.
            pass
        finally:
            self._polling = False

    async def _refresh_open(self) -> None:
        session = self._session
        if session is None or self.host.session_path != session.path:
            return
        try:
            opened = await load_conversation(session)
            if self._session is session and self.host.session_path == session.path:
                self._messages = opened.messages
                self._opened = opened
                self.host.apply_session_update(opened)
        except Exception as exc:
            self.host.notify(f"Could not refresh Codex conversation: {exc}", severity="error")

    async def load_full_message(self, path: str, index: int) -> DisplayMessage | None:
        if self._session is None or self._session.path != path:
            return None
        return self._messages[index] if 0 <= index < len(self._messages) else None
