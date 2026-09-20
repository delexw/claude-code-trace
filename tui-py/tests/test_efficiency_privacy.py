"""Tests for the privacy notice shown before any session data reaches Jev."""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Button, Checkbox, Static

from efficiency import prepared_payload_from_dict
from widgets.efficiency_privacy import (
    EfficiencyPrivacyScreen,
    destination_lines,
    payload_preview,
)

PAYLOAD = prepared_payload_from_dict(
    {
        "sessionId": "s1",
        "sessionPath": "/p/s1.jsonl",
        "payloadMode": "minimized",
        "destination": {
            "provider": "Jev (TypeSafe AI)",
            "endpoint": "https://api.typesafe.ai/v1/decide",
            "model": "jev-1",
        },
        "input": {"task": {"firstUserMessage": "fix the parser", "turns": 4}},
    }
)


def test_destination_lines_name_where_the_data_goes():
    lines = destination_lines(PAYLOAD)
    assert "Provider: Jev (TypeSafe AI)" in lines
    assert "Endpoint: https://api.typesafe.ai/v1/decide" in lines
    assert "Model: jev-1" in lines
    assert "Payload mode: Minimized data" in lines


def test_payload_preview_shows_the_exact_input():
    preview = payload_preview(PAYLOAD)
    assert "fix the parser" in preview
    assert '"turns": 4' in preview


class Harness(App):
    def compose(self) -> ComposeResult:
        yield Static("behind the modal")


@pytest.mark.asyncio
async def test_sending_is_blocked_until_the_notice_is_confirmed():
    async with Harness().run_test() as pilot:
        pilot.app.push_screen(EfficiencyPrivacyScreen(PAYLOAD))
        await pilot.pause()
        screen = pilot.app.screen

        send = screen.query_one("#privacy-send", Button)
        assert send.disabled is True

        screen.query_one("#privacy-confirm", Checkbox).value = True
        await pilot.pause()
        assert send.disabled is False


@pytest.mark.asyncio
async def test_confirming_returns_true_and_cancelling_returns_false():
    async with Harness().run_test() as pilot:
        results: list[bool | None] = []

        pilot.app.push_screen(EfficiencyPrivacyScreen(PAYLOAD), callback=results.append)
        await pilot.pause()
        screen = pilot.app.screen
        screen.query_one("#privacy-confirm", Checkbox).value = True
        await pilot.pause()
        screen.query_one("#privacy-send", Button).press()
        await pilot.pause()
        assert results == [True]

        pilot.app.push_screen(EfficiencyPrivacyScreen(PAYLOAD), callback=results.append)
        await pilot.pause()
        pilot.app.screen.query_one("#privacy-cancel", Button).press()
        await pilot.pause()
        assert results == [True, False]


@pytest.mark.asyncio
async def test_escape_cancels_the_send():
    async with Harness().run_test() as pilot:
        results: list[bool | None] = []
        pilot.app.push_screen(EfficiencyPrivacyScreen(PAYLOAD), callback=results.append)
        await pilot.pause()
        await pilot.press("escape")
        await pilot.pause()
        assert results == [False]


@pytest.mark.asyncio
async def test_a_payload_full_of_brackets_still_renders():
    """Transcripts are full of shell commands and JSON. Rendering the preview
    as markup made a bracketed payload crash the screen."""
    payload = prepared_payload_from_dict(
        {
            "sessionPath": "/p/s1.jsonl",
            "payloadMode": "minimized",
            "destination": {"provider": "Jev [beta]", "endpoint": "https://api/x", "model": "m"},
            "input": {
                "actions": [
                    {"summary": 'grep -rn "x" --include="*.ts" [src] 2>/dev/null | head -40'},
                    {"summary": "[bold red]not a tag[/]"},
                ]
            },
        }
    )

    async with Harness().run_test() as pilot:
        pilot.app.push_screen(EfficiencyPrivacyScreen(payload))
        await pilot.pause()
        # Rendering is where markup is parsed — this is the call that used to raise.
        pilot.app.export_screenshot()

        preview = pilot.app.screen.query_one("#privacy-preview", Static)
        assert "--include=" in preview.content
        assert "[bold red]not a tag[/]" in preview.content
