"""Tests for SessionInfo.recap mapping, mirroring the Rust end-of-session recap field."""

from __future__ import annotations

from data_types import session_info_from_dict


def test_recap_parsed_from_dict():
    info = session_info_from_dict(
        {"path": "p", "session_id": "id", "first_message": "hi", "recap": "did X"}
    )
    assert info.recap == "did X"


def test_recap_defaults_none_when_absent():
    info = session_info_from_dict({"path": "p", "session_id": "id", "first_message": "hi"})
    assert info.recap is None
