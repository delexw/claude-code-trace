"""The terminal UI must hand out the same command as the web UI.

Every case here mirrors one in ``src/plugins/codex/sessionCommand.test.ts``,
with the same inputs and the same expected strings. When you change one file,
change the other.
"""

from __future__ import annotations

import pytest

from plugins.codex.models import CodexSession
from plugins.codex.resume_command import resume_command

ID = "00000000-0000-0000-0000-000000000123"
SESSION = CodexSession(
    id=ID,
    cwd="/work/toy calculator",
    path="/custom/codex/sessions/2026/01/01/rollout-fabricated.jsonl",
)


def command(**changes: str) -> str | None:
    fork = changes.pop("fork", False)
    fields = {"id": SESSION.id, "cwd": SESSION.cwd, "path": SESSION.path, **changes}
    return resume_command(CodexSession(**fields), fork=bool(fork))


def powershell(home: str, cwd: str, session_id: str, action: str = "resume") -> str:
    return (
        "& { $codexTracePreviousHome = $env:CODEX_HOME; try { "
        f"$env:CODEX_HOME = {home}; codex -C {cwd} {action} -- {session_id} "
        "} finally { $env:CODEX_HOME = $codexTracePreviousHome } }"
    )


def test_resumes_the_selected_session_in_its_original_folder_and_custom_home():
    assert command() == (
        f"CODEX_HOME='/custom/codex' codex -C '/work/toy calculator' resume -- '{ID}'"
    )


def test_forks_archived_sessions_with_the_fork_subcommand():
    assert command(path="/custom/codex/archived_sessions/rollout-fabricated.jsonl", fork=True) == (
        f"CODEX_HOME='/custom/codex' codex -C '/work/toy calculator' fork -- '{ID}'"
    )


def test_keeps_shell_metacharacters_literal_in_posix_paths_and_ids():
    assert command(
        cwd="/work/it's $(printf injected); `whoami`",
        id="--last'; printf injected #",
        path="/it's/$(whoami)/sessions/rollout-fabricated.jsonl",
    ) == (
        "CODEX_HOME='/it'\\''s/$(whoami)' codex -C '/work/it'\\''s $(printf injected); `whoami`' "
        "resume -- '--last'\\''; printf injected #'"
    )


def test_a_session_id_that_looks_like_an_option_comes_after_the_separator():
    assert command(id="--last") == (
        "CODEX_HOME='/custom/codex' codex -C '/work/toy calculator' resume -- '--last'"
    )


def test_uses_powershell_quoting_for_native_windows_data():
    assert command(
        cwd="D:\\Toy Projects\\it's $safe`",
        path="C:\\Users\\demo\\custom codex\\sessions\\rollout-fabricated.jsonl",
    ) == powershell(
        "'C:\\Users\\demo\\custom codex'", "'D:\\Toy Projects\\it''s $safe`'", f"'{ID}'"
    )


def test_produces_the_exact_powershell_command_shared_with_the_web_ui():
    assert command(
        cwd="D:\\Toy Projects\\it's here",
        path="C:\\Users\\demo\\custom codex\\sessions\\rollout-fabricated.jsonl",
    ) == (
        "& { $codexTracePreviousHome = $env:CODEX_HOME; try { "
        "$env:CODEX_HOME = 'C:\\Users\\demo\\custom codex'; "
        f"codex -C 'D:\\Toy Projects\\it''s here' resume -- '{ID}' "
        "} finally { $env:CODEX_HOME = $codexTracePreviousHome } }"
    )


# PowerShell closes a single-quoted string at any of these, not only at '.
POWERSHELL_QUOTES = ["'", "\u2018", "\u2019", "\u201a", "\u201b"]


@pytest.mark.parametrize("quote", POWERSHELL_QUOTES, ids=lambda q: f"U+{ord(q):04X}")
def test_doubles_the_powershell_quote_so_a_crafted_value_cannot_close_the_string(quote):
    doubled = quote + quote
    assert command(
        id=f"{quote}; Write-Host id {quote}",
        cwd=f"D:\\a{quote}; Write-Host cwd {quote}b",
        path=f"C:\\h{quote}; Write-Host home {quote}\\sessions\\rollout-fabricated.jsonl",
    ) == powershell(
        f"'C:\\h{doubled}; Write-Host home {doubled}'",
        f"'D:\\a{doubled}; Write-Host cwd {doubled}b'",
        f"'{doubled}; Write-Host id {doubled}'",
    )


def test_doubles_every_quote_in_a_run_of_mixed_powershell_quotes():
    generated = command(
        cwd="D:\\a'\u2018\u2019\u201a\u201bz", path="C:\\h\\sessions\\rollout-fabricated.jsonl"
    )
    assert generated is not None
    assert "codex -C 'D:\\a''\u2018\u2018\u2019\u2019\u201a\u201a\u201b\u201bz' resume" in generated


def test_leaves_curly_quotes_alone_in_posix_commands_where_they_are_ordinary_text():
    assert command(
        cwd="/work/it\u2019s", path="/custom/codex/sessions/rollout-fabricated.jsonl"
    ) == (f"CODEX_HOME='/custom/codex' codex -C '/work/it\u2019s' resume -- '{ID}'")


def test_does_not_map_a_mnt_path_that_contains_a_line_separator_onto_a_drive():
    # JavaScript's `.` stops at U+2028 and U+2029; this must agree with the web UI.
    assert command(cwd="/mnt/c/a\u2028b") == (
        f"CODEX_HOME='/custom/codex' codex -C '/mnt/c/a\u2028b' resume -- '{ID}'"
    )


def test_converts_a_windows_home_read_through_wsl_back_to_its_native_path():
    generated = command(
        cwd="D:\\Toy Projects\\calculator",
        path="/mnt/c/Users/demo/.codex/sessions/rollout-fabricated.jsonl",
    )
    assert generated == powershell(
        "'C:\\Users\\demo\\.codex'", "'D:\\Toy Projects\\calculator'", f"'{ID}'"
    )
    assert "/mnt/c" not in generated


@pytest.mark.parametrize("prefix", ["\\\\wsl.localhost\\ExampleLinux", "\\\\wsl$\\ExampleLinux"])
def test_uses_a_native_posix_home_for_sessions_discovered_through_wsl_unc_paths(prefix):
    generated = command(path=f"{prefix}\\home\\demo\\.codex\\sessions\\rollout-fabricated.jsonl")
    assert generated == (
        f"CODEX_HOME='/home/demo/.codex' codex -C '/work/toy calculator' resume -- '{ID}'"
    )


UNUSABLE = [
    {"cwd": ""},
    {"id": ""},
    {"path": "/unrecognized/rollout.jsonl"},
    {"cwd": "relative"},
    {"cwd": "D:\\toy", "path": "/unmapped/codex/sessions/rollout.jsonl"},
    {"id": "first\nsecond"},
    *[
        change
        for control in ("\0", "\r", "\n")
        for change in (
            {"id": f"abc{control}def"},
            {"cwd": f"/work/a{control}b"},
            {"path": f"/custom/codex/sessions/rollout-a{control}b.jsonl"},
        )
    ],
]


@pytest.mark.parametrize("change", UNUSABLE, ids=repr)
def test_does_not_build_a_command_from_an_unusable_location(change):
    assert command(**change) is None
    assert command(**change, fork=True) is None
