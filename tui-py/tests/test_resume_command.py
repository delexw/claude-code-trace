"""Tests for resume_command — mirrors the TypeScript src/lib/resumeCommand.ts
(same POSIX single-quoting, `'` escaped as `'\\''`)."""

from __future__ import annotations

from resume_command import resume_command


def test_quotes_cwd():
    assert resume_command("/a b", "u1") == "cd '/a b' && claude --resume u1"


def test_fork():
    assert resume_command("/x", "u2", fork=True) == "cd '/x' && claude --resume u2 --fork-session"


def test_escapes_quote():
    assert resume_command("/it's", "u3") == "cd '/it'\\''s' && claude --resume u3"
