"""EfficiencyPrivacyScreen — the notice shown before any session data reaches Jev.

Mirrors the desktop/web modal in src/components/EfficiencyPrivacyModal.tsx: the
same destination, the same list of what may be shared, the same preview of the
exact payload, and the same explicit confirmation — required again for every
analysis, retry, or re-analysis.
"""

from __future__ import annotations

import json

from rich.text import Text
from textual import on
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, Static

import theme
from efficiency import PAYLOAD_MODE_LABELS, PreparedPayload

CONFIRMATION = (
    "I have reviewed the exact data and destination, am authorised to send it, "
    "and accept the third-party processing and risks."
)

SHARED_CONTENT = [
    "Prompts and responses",
    "Tool names, inputs, and results",
    "File paths and commands",
    "Repository or project information",
    "Other content present in the session transcript",
]


def destination_lines(payload: PreparedPayload) -> list[str]:
    """Where the data goes, as plain 'label: value' lines."""
    return [
        f"Provider: {payload.destination.provider}",
        f"Endpoint: {payload.destination.endpoint}",
        f"Model: {payload.destination.model}",
        f"Payload mode: {PAYLOAD_MODE_LABELS.get(payload.payload_mode, payload.payload_mode)}",
        "Transport: HTTPS; your Jev API key is sent as an authorisation credential",
    ]


def payload_preview(payload: PreparedPayload) -> str:
    """The exact analysis input, pretty-printed for review."""
    return json.dumps(payload.raw.get("input", {}), indent=2, ensure_ascii=False)


class EfficiencyPrivacyScreen(ModalScreen[bool]):
    """Dismisses True when the user confirms the send, False otherwise."""

    BINDINGS = [("escape", "cancel", "Cancel")]

    DEFAULT_CSS = """
    EfficiencyPrivacyScreen {
        align: center middle;
    }
    EfficiencyPrivacyScreen #privacy-dialog {
        width: 90;
        max-width: 100%;
        height: 80%;
        border: round $accent;
        background: $surface;
        padding: 1 2;
    }
    EfficiencyPrivacyScreen #privacy-body {
        height: 1fr;
    }
    EfficiencyPrivacyScreen #privacy-actions {
        height: auto;
        align-horizontal: right;
        padding-top: 1;
    }
    EfficiencyPrivacyScreen Button {
        margin-left: 2;
    }
    """

    def __init__(self, payload: PreparedPayload, **kwargs) -> None:
        super().__init__(**kwargs)
        self._payload = payload

    def compose(self) -> ComposeResult:
        with Vertical(id="privacy-dialog"):
            yield Static(
                f"[bold {theme.ACCENT}]Privacy notice[/]  "
                f"[{theme.TEXT_DIM}]before sharing session data with Jev[/]"
            )
            with VerticalScroll(id="privacy-body"):
                yield Static(
                    f"[bold {theme.TOKEN_HIGH}]Your session data will leave this device.[/] "
                    "This analysis sends selected data from this Claude Code session to Jev, "
                    "an external AI service operated by TypeSafe AI."
                )
                yield Static("\n[bold]Where your data goes[/]")
                yield Static(self._destination_text())
                yield Static("\n[bold]What may be shared[/]")
                yield Static(self._shared_text())
                yield Static(
                    f"\n[{theme.TEXT_DIM}]Claude Code Trace minimises and redacts the payload "
                    "locally where possible, but sensitive information may still be included. "
                    "Only send data you own or are authorised to disclose. Jev results are "
                    "automated and may be inaccurate. This notice appears again for every "
                    "analysis, retry, or re-analysis.[/]"
                )
                yield Static("\n[bold]The exact data being sent[/]")
                # markup=False: the payload is arbitrary session content, and
                # a bracket in it would otherwise be read as a markup tag.
                yield Static(payload_preview(self._payload), markup=False, id="privacy-preview")
            yield Checkbox(CONFIRMATION, id="privacy-confirm")
            with Horizontal(id="privacy-actions"):
                yield Button("Cancel", id="privacy-cancel")
                yield Button("Send to Jev", variant="primary", id="privacy-send", disabled=True)

    def _destination_text(self) -> Text:
        """Built as a Text, not markup: the endpoint and model come from the
        backend and must never be parsed as markup tags."""
        lines = "\n".join(f"  {line}" for line in destination_lines(self._payload))
        return Text(lines, style=theme.TEXT_DIM)

    def _shared_text(self) -> Text:
        lines = "\n".join(f"  · {item}" for item in SHARED_CONTENT)
        return Text(lines, style=theme.TEXT_DIM)

    @on(Checkbox.Changed, "#privacy-confirm")
    def _toggle_send(self, event: Checkbox.Changed) -> None:
        self.query_one("#privacy-send", Button).disabled = not event.value

    @on(Button.Pressed, "#privacy-send")
    def _send(self) -> None:
        self.dismiss(True)

    @on(Button.Pressed, "#privacy-cancel")
    def _cancel_pressed(self) -> None:
        self.dismiss(False)

    def action_cancel(self) -> None:
        self.dismiss(False)
