"""Read-only Codex chat source for the terminal UI."""

from __future__ import annotations

import api as api_client
from data_types import DisplayMessage, SessionMeta, SessionTotals, message_from_dict
from plugins.base import LIST_VIEW, PICKER_VIEW, OpenedSession, PickerSection, SourcePlugin
from plugins.codex.models import CodexSession, session_from_dict
from plugins.codex.picker_rows import build_section


async def load_conversation(session: CodexSession) -> OpenedSession:
    data = await api_client.post_json("/api/codex/session", {"path": session.path})
    if not isinstance(data, dict):
        raise ValueError("Invalid Codex session response")
    messages = [message_from_dict(raw) for raw in data.get("messages", []) if isinstance(raw, dict)]
    for message in messages:
        message.role_label = "Codex" if message.role == "claude" else message.role.title()
    return OpenedSession(
        path=session.path,
        messages=messages,
        ongoing=bool(data.get("ongoing", False)),
        meta=SessionMeta(cwd=session.cwd),
        totals=SessionTotals(
            input_tokens=int(data.get("input_tokens", 0)),
            output_tokens=int(data.get("output_tokens", 0)),
            cache_read_tokens=int(data.get("cache_read_tokens", 0)),
            total_tokens=int(data.get("total_tokens", 0)),
            cost_usd=float(data.get("cost_usd") or 0),
            model=session.model,
        ),
    )


class CodexPlugin(SourcePlugin):
    id = "codex"
    label = "Codex"

    def __init__(self, host) -> None:
        super().__init__(host)
        self.sessions: list[CodexSession] = []
        self.loading = True
        self.error = ""
        self._session: CodexSession | None = None
        self._messages: list[DisplayMessage] = []

    def start(self) -> None:
        self.host.run_worker(self._fetch_sessions(), exclusive=True, group="codex-picker")

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
        if self.host.view == PICKER_VIEW:
            self.host.sync_picker()

    def picker_section(self) -> PickerSection:
        return build_section(self.sessions, loading=self.loading, error=self.error)

    def open(self, item: object) -> None:
        if isinstance(item, CodexSession):
            self.host.run_worker(
                self._load(item), exclusive=True, group="load_session", name="load_session"
            )

    async def _load(self, session: CodexSession) -> None:
        self.host.set_picker_busy(True)
        try:
            opened = await load_conversation(session)
            self._session, self._messages = session, opened.messages
            await self.host.present_session(self, opened)
            self.host.view = LIST_VIEW
        except Exception as exc:
            self.host.notify(f"Could not open Codex conversation: {exc}", severity="error")
        finally:
            self.host.set_picker_busy(False)

    async def _refresh_open(self) -> None:
        session = self._session
        if session is None or self.host.session_path != session.path:
            return
        try:
            opened = await load_conversation(session)
            if self._session is session and self.host.session_path == session.path:
                self._messages = opened.messages
                self.host.apply_session_update(opened)
        except Exception as exc:
            self.host.notify(f"Could not refresh Codex conversation: {exc}", severity="error")

    async def load_full_message(self, path: str, index: int) -> DisplayMessage | None:
        if self._session is None or self._session.path != path:
            return None
        return self._messages[index] if 0 <= index < len(self._messages) else None
