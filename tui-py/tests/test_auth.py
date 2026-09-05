"""Tests for auth — how the TUI finds the shared HTTP API client token."""

from __future__ import annotations

from pathlib import Path

import auth


def test_config_dir_linux_prefers_xdg_config_home():
    assert auth.config_dir("linux", {"XDG_CONFIG_HOME": "/xdg"}, Path("/h")) == Path("/xdg")


def test_config_dir_linux_falls_back_to_dot_config():
    assert auth.config_dir("linux", {}, Path("/h")) == Path("/h/.config")


def test_config_dir_darwin_uses_application_support():
    assert auth.config_dir("darwin", {}, Path("/Users/x")) == Path(
        "/Users/x/Library/Application Support"
    )


def test_config_dir_windows_uses_appdata_with_fallback():
    assert auth.config_dir("win32", {"APPDATA": "C:/appdata"}, Path("C:/u")) == Path("C:/appdata")
    assert auth.config_dir("win32", {}, Path("/u")) == Path("/u/AppData/Roaming")


def test_token_path_is_inside_app_config_dir():
    assert auth.token_path("linux", {}, Path("/h")) == Path(
        "/h/.config/claude-code-trace/api-token"
    )


def test_app_config_root_honours_override_env():
    env = {"CCTRACE_CONFIG_DIR": " /e2e/cfg ", "XDG_CONFIG_HOME": "/ignored"}
    assert auth.app_config_root("linux", env, Path("/h")) == Path("/e2e/cfg")
    assert auth.token_path("linux", env, Path("/h")) == Path("/e2e/cfg/api-token")
    assert auth.app_config_root("linux", {"CCTRACE_CONFIG_DIR": "  "}, Path("/h")) == Path(
        "/h/.config/claude-code-trace"
    )


def test_resolve_env_off_wins(tmp_path: Path):
    p = tmp_path / "api-token"
    p.write_text("filetoken\n")
    assert (
        auth.resolve_api_token({"CCTRACE_API_AUTH": " OFF ", "CCTRACE_API_TOKEN": "x"}, p) is None
    )


def test_resolve_env_token_wins_over_file(tmp_path: Path):
    p = tmp_path / "api-token"
    p.write_text("filetoken\n")
    assert auth.resolve_api_token({"CCTRACE_API_TOKEN": "  envtoken "}, p) == "envtoken"


def test_resolve_reads_and_strips_file(tmp_path: Path):
    p = tmp_path / "api-token"
    p.write_text("  abc123 \n")
    assert auth.resolve_api_token({}, p) == "abc123"


def test_resolve_missing_or_empty_file_is_none(tmp_path: Path):
    assert auth.resolve_api_token({}, tmp_path / "missing") is None
    empty = tmp_path / "empty"
    empty.write_text("\n")
    assert auth.resolve_api_token({}, empty) is None


def test_resolve_defaults_to_config_dir_path(tmp_path: Path, monkeypatch):
    token_file = tmp_path / "claude-code-trace" / "api-token"
    token_file.parent.mkdir()
    token_file.write_text("fromdefault\n")
    monkeypatch.setattr(auth.sys, "platform", "linux")
    assert auth.resolve_api_token({"XDG_CONFIG_HOME": str(tmp_path)}) == "fromdefault"


def test_auth_headers_uses_explicit_token():
    assert auth.auth_headers("t") == {"X-CCTrace-Token": "t"}


def test_auth_headers_empty_without_token(monkeypatch):
    monkeypatch.setattr(auth, "resolve_api_token", lambda: None)
    assert auth.auth_headers() == {}


def test_auth_headers_resolves_when_no_token_given(monkeypatch):
    monkeypatch.setattr(auth, "resolve_api_token", lambda: "resolved")
    assert auth.auth_headers() == {"X-CCTrace-Token": "resolved"}
