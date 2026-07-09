"""Tests for the nested `liveness` object mapping in session_info_from_dict."""

from __future__ import annotations

from data_types import session_info_from_dict


def test_liveness_parsed_from_nested_dict():
    info = session_info_from_dict(
        {
            "path": "p",
            "session_id": "id",
            "first_message": "hi",
            "liveness": {"status": "idle", "idle_seconds": 60, "pid": 123},
        }
    )
    assert info.liveness is not None
    assert info.liveness.status == "idle"
    assert info.liveness.idle_seconds == 60
    assert info.liveness.pid == 123


def test_liveness_degrades_to_none_on_malformed_shape():
    info = session_info_from_dict(
        {
            "path": "p",
            "session_id": "id",
            "first_message": "hi",
            "liveness": {"status": "idle", "idle_seconds": "notanumber"},
        }
    )
    assert info.liveness is None
