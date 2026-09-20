"""AnalyticsSettingsScreen — Jev and recommendation-provider settings for the TUI.

The TUI reaches the backend over HTTP, exactly as the browser does, so it has
the same limits as web mode: API keys cannot be entered here (the HTTP API
does not accept them), and an OpenAI-compatible endpoint must be a local,
unauthenticated one. Everything else — the default payload mode and the
recommendation provider — is read and written through /api/analytics/settings.
"""

from __future__ import annotations

from rich.text import Text
from textual import on
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Input, RadioButton, RadioSet, Static

import api as api_client
import theme
from efficiency import (
    CLAUDE_CODE_SUBSCRIPTION,
    CODEX_SUBSCRIPTION,
    DEFAULT_OPENAI_BASE_URL,
    OPENAI_COMPATIBLE,
    PAYLOAD_MODE_LABELS,
    PAYLOAD_MODES,
    PROVIDER_LABELS,
    AnalyticsSettings,
    RecommendationProvider,
    jev_status_label,
)

KEY_ENTRY_NOTE = (
    "API keys cannot be entered from the TUI — it talks to the backend over HTTP, "
    "which never accepts them. Set JEV_API_KEY before starting the backend, or use "
    "the desktop app to store the key in your operating system credential store."
)

DOCKER_PROVIDER_NOTE = (
    "Subscription providers are unavailable in Docker because the container does not "
    "include or authenticate the Codex and Claude Code CLIs. Use a local, "
    "unauthenticated OpenAI-compatible endpoint."
)

LOOPBACK_NOTE = (
    "Only a loopback endpoint such as localhost or 127.0.0.1 is accepted, with no "
    "credentials or token query parameters in the URL."
)


def _mode_id(mode: str) -> str:
    return f"mode-{mode}"


def _provider_id(kind: str) -> str:
    return f"provider-{kind}"


class AnalyticsSettingsScreen(ModalScreen[None]):
    """Modal over the session list. Built from settings already loaded by the
    caller, so every control shows its real value on first paint."""

    BINDINGS = [("escape", "close", "Close")]

    DEFAULT_CSS = """
    AnalyticsSettingsScreen {
        align: center middle;
    }
    AnalyticsSettingsScreen #settings-dialog {
        width: 84;
        max-width: 100%;
        height: 80%;
        border: round $accent;
        background: $surface;
        padding: 1 2;
    }
    AnalyticsSettingsScreen #settings-body {
        height: 1fr;
    }
    AnalyticsSettingsScreen #settings-actions {
        height: auto;
        align-horizontal: right;
        padding-top: 1;
    }
    AnalyticsSettingsScreen Button {
        margin-left: 2;
    }
    AnalyticsSettingsScreen Input {
        margin-bottom: 1;
    }
    """

    def __init__(self, settings: AnalyticsSettings, **kwargs) -> None:
        super().__init__(**kwargs)
        self._settings = settings

    # ----------------------------------------------------------------
    # Layout
    # ----------------------------------------------------------------

    def compose(self) -> ComposeResult:
        provider = self._settings.recommendation_provider
        with Vertical(id="settings-dialog"):
            yield Static(f"[bold {theme.ACCENT}]Analytics settings[/]")
            with VerticalScroll(id="settings-body"):
                yield Static("\n[bold]Session efficiency analysis (Jev)[/]")
                yield Static(self._jev_status_text(), id="jev-status")
                yield Static(f"[{theme.TEXT_DIM}]{KEY_ENTRY_NOTE}[/]")
                yield Button(
                    "Test Jev connection",
                    id="test-jev",
                    disabled=not self._settings.jev.configured,
                )

                yield Static("\n[bold]Default payload mode[/]")
                yield RadioSet(
                    *(
                        RadioButton(
                            PAYLOAD_MODE_LABELS[mode],
                            id=_mode_id(mode),
                            value=mode == self._settings.default_payload_mode,
                        )
                        for mode in PAYLOAD_MODES
                    ),
                    id="payload-mode",
                )

                yield Static("\n[bold]Improvement recommendations[/]")
                yield Static(
                    f"[{theme.TEXT_DIM}]Configured independently from Jev, and used only to "
                    "generate recommendations.[/]"
                )
                if not self._settings.subscription_providers_available:
                    yield Static(f"[{theme.TOKEN_HIGH}]{DOCKER_PROVIDER_NOTE}[/]")
                yield RadioSet(
                    *(
                        RadioButton(
                            PROVIDER_LABELS[kind],
                            id=_provider_id(kind),
                            value=kind == provider.kind,
                        )
                        for kind in self._available_provider_kinds()
                    ),
                    id="provider-kind",
                )
                yield Static(f"[{theme.TEXT_DIM}]{LOOPBACK_NOTE}[/]", id="loopback-note")
                yield Static("Base URL")
                yield Input(value=provider.base_url, id="provider-base-url")
                yield Static("Model")
                yield Input(
                    value=provider.model,
                    placeholder="default / auto",
                    id="provider-model",
                )
                yield Button("Test recommendation provider", id="test-provider")
            with Horizontal(id="settings-actions"):
                yield Button("Close", id="settings-close")
                yield Button("Save", variant="primary", id="settings-save")

    def on_mount(self) -> None:
        self._sync_provider_fields(self._settings.recommendation_provider.kind)

    def _available_provider_kinds(self) -> list[str]:
        if self._settings.subscription_providers_available:
            return [CODEX_SUBSCRIPTION, CLAUDE_CODE_SUBSCRIPTION, OPENAI_COMPATIBLE]
        return [OPENAI_COMPATIBLE]

    # ----------------------------------------------------------------
    # Reading the form
    # ----------------------------------------------------------------

    def _selected_payload_mode(self) -> str:
        pressed = self.query_one("#payload-mode", RadioSet).pressed_button
        for mode in PAYLOAD_MODES:
            if pressed is not None and pressed.id == _mode_id(mode):
                return mode
        return self._settings.default_payload_mode

    def _selected_provider_kind(self) -> str:
        pressed = self.query_one("#provider-kind", RadioSet).pressed_button
        for kind in self._available_provider_kinds():
            if pressed is not None and pressed.id == _provider_id(kind):
                return kind
        return self._settings.recommendation_provider.kind

    def provider_from_form(self) -> RecommendationProvider:
        """The provider the form currently describes."""
        kind = self._selected_provider_kind()
        model = self.query_one("#provider-model", Input).value.strip()
        if kind != OPENAI_COMPATIBLE:
            return RecommendationProvider(kind=kind, model=model)
        loaded = self._settings.recommendation_provider
        return RecommendationProvider(
            kind=kind,
            model=model,
            base_url=self.query_one("#provider-base-url", Input).value.strip(),
            # Whether a key is stored is the backend's fact, not the form's —
            # carry it through untouched, and only when it belongs to this
            # provider variant.
            api_key_configured=loaded.kind == OPENAI_COMPATIBLE and loaded.api_key_configured,
        )

    def _sync_provider_fields(self, kind: str) -> None:
        """Base URL and the loopback note only apply to the OpenAI-compatible variant."""
        openai_compatible = kind == OPENAI_COMPATIBLE
        for node_id in ("#loopback-note", "#provider-base-url"):
            self.query_one(node_id).display = openai_compatible

    @on(RadioSet.Changed, "#provider-kind")
    def _provider_changed(self, _event: RadioSet.Changed) -> None:
        kind = self._selected_provider_kind()
        self._sync_provider_fields(kind)
        base_url = self.query_one("#provider-base-url", Input)
        if kind == OPENAI_COMPATIBLE and not base_url.value.strip():
            base_url.value = DEFAULT_OPENAI_BASE_URL

    # ----------------------------------------------------------------
    # Actions
    # ----------------------------------------------------------------

    @on(Button.Pressed, "#settings-save")
    def _save_pressed(self) -> None:
        self.run_worker(self._save(), exclusive=True, group="analytics-settings")

    async def _save(self) -> None:
        try:
            saved = await api_client.set_analytics_settings(
                self._selected_payload_mode(), self.provider_from_form()
            )
        except Exception as error:
            self.notify(str(error), title="Could not save settings", severity="error", markup=False)
            return
        self._settings = saved
        self._refresh_jev_status()
        self.notify("Analytics settings saved.", markup=False)

    @on(Button.Pressed, "#test-jev")
    def _test_jev_pressed(self) -> None:
        self.run_worker(self._test_jev(), exclusive=True, group="analytics-test")

    async def _test_jev(self) -> None:
        try:
            await api_client.test_jev_connection()
        except Exception as error:
            self.notify(str(error), title="Jev connection failed", severity="error", markup=False)
            return
        self.notify("Jev connection successful.", markup=False)

    @on(Button.Pressed, "#test-provider")
    def _test_provider_pressed(self) -> None:
        self.run_worker(self._test_provider(), exclusive=True, group="analytics-test")

    async def _test_provider(self) -> None:
        try:
            await api_client.test_recommendation_provider(self.provider_from_form())
        except Exception as error:
            self.notify(str(error), title="Provider test failed", severity="error", markup=False)
            return
        self.notify("Recommendation provider connected.", markup=False)

    def _jev_status_text(self) -> Text:
        """A Text, not markup — the status word comes from the backend."""
        return Text(f"Status: {jev_status_label(self._settings.jev)}", style=theme.TEXT_DIM)

    def _refresh_jev_status(self) -> None:
        self.query_one("#jev-status", Static).update(self._jev_status_text())

    @on(Button.Pressed, "#settings-close")
    def _close_pressed(self) -> None:
        self.dismiss(None)

    def action_close(self) -> None:
        self.dismiss(None)
