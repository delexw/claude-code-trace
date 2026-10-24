"""The Google Antigravity (Gemini IDE) session source: read-only brain conversations."""

from __future__ import annotations

import api as api_client
from data_types import DisplayMessage, SessionMeta, SessionTotals
from plugins.antigravity.convert import to_display_message
from plugins.antigravity.models import BrainFolder, folder_from_dict, step_from_dict
from plugins.antigravity.picker_rows import build_section
from plugins.base import LIST_VIEW, PICKER_VIEW, OpenedSession, PickerSection, SourcePlugin


async def list_folders() -> list[BrainFolder]:
    data = await api_client.get_json("/api/antigravity/folders")
    return (
        [folder_from_dict(d) for d in data if isinstance(d, dict)] if isinstance(data, list) else []
    )


async def load_conversation(path: str) -> list[DisplayMessage]:
    data = await api_client.post_json("/api/antigravity/session", {"path": path})
    raw = data.get("messages", []) if isinstance(data, dict) else []
    return [to_display_message(step_from_dict(d)) for d in raw if isinstance(d, dict)]


class AntigravityPlugin(SourcePlugin):
    """Conversations under Antigravity's brain folders. Off until enabled in Settings."""

    id = "antigravity"
    label = "Antigravity"

    def __init__(self, host) -> None:
        super().__init__(host)
        self.folders: list[BrainFolder] = []
        self.loading = True
        self.error = ""
        self._messages: list[DisplayMessage] = []

    def start(self) -> None:
        self.host.run_worker(self._fetch_folders(), exclusive=False, name="antigravity-folders")

    def refresh(self) -> None:
        self.start()

    async def _fetch_folders(self) -> None:
        try:
            folders, error = await list_folders(), ""
        except Exception as exc:
            folders, error = self.folders, f"Could not list Antigravity conversations: {exc}"
        changed = (folders, error, False) != (self.folders, self.error, self.loading)
        self.folders, self.error, self.loading = folders, error, False
        if changed and self.host.view == PICKER_VIEW:
            self.host.sync_picker()

    def picker_section(self) -> PickerSection:
        return build_section(self.folders, loading=self.loading, error=self.error)

    def open(self, item: object) -> None:
        if not isinstance(item, BrainFolder):
            return
        self.host.run_worker(
            self._load(item), exclusive=True, group="load_session", name="load_session"
        )

    async def _load(self, folder: BrainFolder) -> None:
        self.host.set_picker_busy(True)
        try:
            self._messages = await load_conversation(folder.path)
            await self.host.present_session(
                self,
                OpenedSession(
                    path=folder.path,
                    messages=self._messages,
                    ongoing=False,
                    meta=SessionMeta(cwd=folder.path),
                    totals=SessionTotals(),
                ),
            )
            self.host.view = LIST_VIEW
        except Exception as exc:
            self.host.notify(f"Could not open the conversation: {exc}", severity="error")
        finally:
            self.host.set_picker_busy(False)

    async def load_full_message(self, path: str, index: int) -> DisplayMessage | None:
        # The whole conversation is loaded on open, bodies included.
        return self._messages[index] if 0 <= index < len(self._messages) else None
