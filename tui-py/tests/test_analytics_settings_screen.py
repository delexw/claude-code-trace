"""Tests for the TUI's analytics settings screen."""

from __future__ import annotations

import pytest
from textual.app import App, ComposeResult
from textual.widgets import Button, Input, RadioSet, Static

import api
from efficiency import (
    CLAUDE_CODE_SUBSCRIPTION,
    CODEX_SUBSCRIPTION,
    OPENAI_COMPATIBLE,
    AnalyticsSettings,
    JevStatus,
    RecommendationProvider,
)
from widgets.analytics_settings import AnalyticsSettingsScreen

CONFIGURED = AnalyticsSettings(
    jev=JevStatus(configured=True, source="environment", status="configured"),
    default_payload_mode="minimized",
    recommendation_provider=RecommendationProvider(kind=CODEX_SUBSCRIPTION, model="gpt-5-codex"),
    subscription_providers_available=True,
)

DOCKER = AnalyticsSettings(
    jev=JevStatus(configured=False, source=None, status="not_configured"),
    default_payload_mode="full-transcript",
    recommendation_provider=RecommendationProvider(
        kind=OPENAI_COMPATIBLE,
        base_url="http://localhost:1234/v1",
        model="qwen",
        api_key_configured=True,
    ),
    subscription_providers_available=False,
)


class Harness(App):
    def compose(self) -> ComposeResult:
        yield Static("behind the modal")


async def _open(pilot, settings: AnalyticsSettings) -> AnalyticsSettingsScreen:
    screen = AnalyticsSettingsScreen(settings)
    pilot.app.push_screen(screen)
    await pilot.pause()
    return screen


@pytest.mark.asyncio
async def test_opens_showing_the_loaded_values():
    async with Harness().run_test() as pilot:
        screen = await _open(pilot, CONFIGURED)

        assert "Configured via environment" in screen.query_one("#jev-status", Static).content
        assert screen.query_one("#payload-mode", RadioSet).pressed_button.id == "mode-minimized"
        assert (
            screen.query_one("#provider-kind", RadioSet).pressed_button.id
            == f"provider-{CODEX_SUBSCRIPTION}"
        )
        assert screen.query_one("#provider-model", Input).value == "gpt-5-codex"


@pytest.mark.asyncio
async def test_the_base_url_field_is_only_shown_for_an_openai_compatible_endpoint():
    async with Harness().run_test() as pilot:
        screen = await _open(pilot, CONFIGURED)
        assert screen.query_one("#provider-base-url", Input).display is False

        screen.query_one(f"#provider-{OPENAI_COMPATIBLE}").value = True
        await pilot.pause()

        base_url = screen.query_one("#provider-base-url", Input)
        assert base_url.display is True
        # An empty field would fail the backend's URL check — offer the local default.
        assert base_url.value == "http://localhost:1234/v1"


@pytest.mark.asyncio
async def test_jev_test_button_is_disabled_without_a_key():
    async with Harness().run_test() as pilot:
        screen = await _open(pilot, DOCKER)
        assert screen.query_one("#test-jev", Button).disabled is True


@pytest.mark.asyncio
async def test_docker_only_offers_the_openai_compatible_provider():
    async with Harness().run_test() as pilot:
        screen = await _open(pilot, DOCKER)
        offered = [
            button.id
            for button in screen.query_one("#provider-kind", RadioSet).query("RadioButton")
        ]
        assert offered == [f"provider-{OPENAI_COMPATIBLE}"]


@pytest.mark.asyncio
async def test_saving_posts_the_selected_mode_and_provider(monkeypatch):
    saved: list[tuple[str, dict]] = []

    async def fake_set(payload_mode, provider):
        saved.append((payload_mode, provider.to_dict()))
        return AnalyticsSettings(
            jev=CONFIGURED.jev,
            default_payload_mode=payload_mode,
            recommendation_provider=provider,
        )

    monkeypatch.setattr(api, "set_analytics_settings", fake_set)

    async with Harness().run_test() as pilot:
        screen = await _open(pilot, CONFIGURED)
        screen.query_one("#mode-full-transcript").value = True
        screen.query_one(f"#provider-{CLAUDE_CODE_SUBSCRIPTION}").value = True
        screen.query_one("#provider-model", Input).value = "claude-opus-5"
        await pilot.pause()

        screen.query_one("#settings-save", Button).press()
        await pilot.pause()

        assert saved == [
            ("full-transcript", {"type": CLAUDE_CODE_SUBSCRIPTION, "model": "claude-opus-5"})
        ]


@pytest.mark.asyncio
async def test_saving_keeps_a_stored_provider_key_the_tui_cannot_see():
    """`apiKeyConfigured` is the backend's fact — saving the form must not
    clear a key the TUI has no way to re-enter."""
    async with Harness().run_test() as pilot:
        screen = await _open(pilot, DOCKER)
        assert screen.provider_from_form().to_dict() == {
            "type": OPENAI_COMPATIBLE,
            "baseUrl": "http://localhost:1234/v1",
            "model": "qwen",
            "apiKeyConfigured": True,
        }


@pytest.mark.asyncio
async def test_switching_away_from_the_endpoint_drops_its_key_flag():
    async with Harness().run_test() as pilot:
        screen = await _open(pilot, CONFIGURED)
        screen.query_one(f"#provider-{OPENAI_COMPATIBLE}").value = True
        await pilot.pause()
        assert screen.provider_from_form().api_key_configured is False


@pytest.mark.asyncio
async def test_a_failed_save_is_reported_with_the_backend_s_own_words(monkeypatch):
    async def fake_set(_payload_mode, _provider):
        raise RuntimeError("Web mode supports only a local OpenAI-compatible endpoint")

    monkeypatch.setattr(api, "set_analytics_settings", fake_set)

    async with Harness().run_test() as pilot:
        notices: list[tuple[str, str]] = []
        pilot.app.notify = lambda message, **kwargs: notices.append(
            (message, kwargs.get("severity", "information"))
        )
        screen = await _open(pilot, CONFIGURED)
        screen.query_one("#settings-save", Button).press()
        await pilot.pause()
        await pilot.pause()

        assert notices == [("Web mode supports only a local OpenAI-compatible endpoint", "error")]


@pytest.mark.asyncio
async def test_escape_closes_the_screen():
    async with Harness().run_test() as pilot:
        await _open(pilot, CONFIGURED)
        assert isinstance(pilot.app.screen, AnalyticsSettingsScreen)
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(pilot.app.screen, AnalyticsSettingsScreen)


@pytest.mark.asyncio
async def test_a_bracketed_status_from_the_backend_still_renders():
    settings = AnalyticsSettings(jev=JevStatus(configured=False, status="error [401]"))
    async with Harness().run_test() as pilot:
        await _open(pilot, settings)
        pilot.app.export_screenshot()
        assert "Error [401]" in pilot.app.screen.query_one("#jev-status", Static).content
