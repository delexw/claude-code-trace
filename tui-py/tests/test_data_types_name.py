"""Tests for SessionInfo.name mapping, mirroring the Rust registry-join tests."""

from __future__ import annotations

from data_types import session_info_from_dict


def test_session_info_maps_name():
    info = session_info_from_dict({"session_id": "a", "name": "my-cache"})
    assert info.name == "my-cache"


def test_session_info_name_defaults_to_empty_when_absent():
    info = session_info_from_dict({"session_id": "a"})
    assert info.name == ""


def test_session_info_name_null_becomes_empty():
    # The API serializes an unnamed session's name as JSON null.
    info = session_info_from_dict({"session_id": "a", "name": None})
    assert info.name == ""
