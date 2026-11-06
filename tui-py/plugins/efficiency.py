"""Shared source adapter for Jev state, local preview and explicit per-send consent."""

from __future__ import annotations

from collections.abc import Iterable
from typing import Protocol
from weakref import WeakKeyDictionary

import api as api_client
from efficiency import EfficiencyJob, EfficiencySummary, job_from_dict, latest_jobs_by_session
from plugins.base import SourcePlugin
from widgets.analytics_settings import AnalyticsSettingsScreen
from widgets.efficiency_privacy import EfficiencyPrivacyScreen

FAILURE_NOTICE_SECONDS = 30
JEV_KEY_REQUIRED = (
    "Set JEV_API_KEY before starting the backend, or store the key with the desktop "
    "app, then reopen the analytics settings to confirm it."
)
# Every enabled source receives the shared job stream; toast each failure once per app.
_notified_failures: WeakKeyDictionary[object, set[tuple[str, str, str]]] = WeakKeyDictionary()


class EfficiencySession(Protocol):
    path: str


class EfficiencySourcePlugin(SourcePlugin):
    """A source supplies its sessions, local prepare endpoint and display label."""

    efficiency_prepare_endpoint: str | None = None
    efficiency_source_label: str = ""

    def __init__(self, host) -> None:
        super().__init__(host)
        self.efficiency_jobs: dict[str, EfficiencyJob] = {}
        self.efficiency_summaries: dict[str, EfficiencySummary] = {}

    def efficiency_sessions(self) -> Iterable[EfficiencySession]:
        return ()

    async def on_efficiency_update(self, payload) -> None:
        job = job_from_dict(payload or {})
        if not job.session_path:
            return
        self._set_jobs({**self.efficiency_jobs, job.session_path: job})
        if job.status == "completed":
            await self._refresh_efficiency_summaries()
        elif job.status == "failed":
            seen = _notified_failures.setdefault(self.host, set())
            token = (job.analysis_id or job.session_path, job.updated_at, job.error or job.message)
            if token in seen:
                return
            seen.add(token)
            name = job.session_name or job.session_id or job.session_path
            self.host.notify(
                f"{name}: {job.error or job.message}",
                title="Jev analysis failed",
                severity="error",
                timeout=FAILURE_NOTICE_SECONDS,
                markup=False,
            )

    async def load_efficiency_state(self) -> None:
        try:
            jobs = await api_client.list_efficiency_jobs()
            summaries = await api_client.list_efficiency_summaries()
        except Exception:
            return
        self._set_jobs(latest_jobs_by_session(jobs))
        self._set_summaries({summary.session_path: summary for summary in summaries})

    async def _refresh_efficiency_summaries(self) -> None:
        try:
            summaries = await api_client.list_efficiency_summaries()
        except Exception:
            return
        self._set_summaries({summary.session_path: summary for summary in summaries})

    def _set_jobs(self, jobs: dict[str, EfficiencyJob]) -> None:
        if jobs != self.efficiency_jobs:
            self.efficiency_jobs = jobs
            self._sync_picker_analysis()

    def _set_summaries(self, summaries: dict[str, EfficiencySummary]) -> None:
        if summaries != self.efficiency_summaries:
            self.efficiency_summaries = summaries
            self._sync_picker_analysis()

    def _sync_picker_analysis(self) -> None:
        self.host.refresh_picker_rows(
            self.id,
            [
                session
                for session in self.efficiency_sessions()
                if session.path in self.efficiency_jobs or session.path in self.efficiency_summaries
            ],
        )

    async def analyse_session(self, session: EfficiencySession) -> None:
        """Prepare locally, then ask for the exact payload's consent on every send."""
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
            if self.efficiency_prepare_endpoint is None:
                payload = await api_client.prepare_efficiency_payload(
                    session.path, settings.default_payload_mode
                )
            else:
                payload = await api_client.prepare_efficiency_payload(
                    session.path,
                    settings.default_payload_mode,
                    endpoint=self.efficiency_prepare_endpoint,
                )
        except Exception as error:
            self.host.notify(
                str(error), title="Could not prepare the analysis", severity="error", markup=False
            )
            return
        if not await self.host.push_screen_wait(
            EfficiencyPrivacyScreen(
                payload, source_label=self.efficiency_source_label or self.label
            )
        ):
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

    def action_analyse_session(self) -> None:
        session = self.host.highlighted_picker_item(self.id)
        if session is not None and getattr(session, "path", ""):
            self.host.run_worker(self.analyse_session(session), exclusive=True, group="analyse")

    def action_analytics_settings(self) -> None:
        self.host.run_worker(
            self._open_analytics_settings(), exclusive=True, group="analytics-settings"
        )
