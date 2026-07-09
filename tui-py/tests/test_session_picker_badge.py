"""Tests for the liveness badge text rendered in the session picker."""

from __future__ import annotations

import theme
from data_types import Liveness, SessionInfo
from widgets.session_picker import _liveness_badge, _render_session


def test_liveness_badge_busy():
    assert _liveness_badge(Liveness(status="busy", idle_seconds=0, pid=1)) == "● busy"


def test_liveness_badge_idle_shows_minutes():
    assert _liveness_badge(Liveness(status="idle", idle_seconds=180, pid=1)) == "○ idle 3m"


def test_liveness_badge_unknown_status_falls_back():
    assert _liveness_badge(Liveness(status="compacting", idle_seconds=0, pid=1)) == "○ compacting"


# ---------------------------------------------------------------------------
# _render_session — the badge splice into line 2
# ---------------------------------------------------------------------------


def _spans_with(text, needle):
    """Return (covered_text, style) for spans whose style contains `needle`."""
    return [
        (text.plain[s.start : s.end], str(s.style)) for s in text.spans if needle in str(s.style)
    ]


def _content_text(session: SessionInfo):
    """The rendered `Text` (line1 [+ subtitle] + line2) for `session`, pulled
    out of the `Group(content, sep)` returned by `_render_session`."""
    group = _render_session(session)
    content = group.renderables[0]
    assert content.plain  # sanity: it's the assembled Text, not the Rule
    return content


def test_render_session_busy_shows_badge_with_ongoing_style():
    session = SessionInfo(
        session_id="sid-busy12345",
        liveness=Liveness(status="busy", idle_seconds=0, pid=123),
    )
    text = _content_text(session)
    assert "● busy" in text.plain
    matches = _spans_with(text, theme.ONGOING)
    assert any("● busy" in covered for covered, _ in matches), (
        "busy badge must be styled with theme.ONGOING"
    )


def test_render_session_idle_shows_badge_with_dim_style():
    session = SessionInfo(
        session_id="sid-idle12345",
        liveness=Liveness(status="idle", idle_seconds=120, pid=123),
    )
    text = _content_text(session)
    assert "○ idle 2m" in text.plain
    matches = _spans_with(text, theme.TEXT_DIM)
    assert any("○ idle 2m" in covered for covered, _ in matches), (
        "idle badge must be styled with theme.TEXT_DIM (not the ongoing/busy style)"
    )


def test_render_session_no_liveness_omits_badge():
    session = SessionInfo(session_id="sid-none12345", liveness=None)
    text = _content_text(session)
    assert "busy" not in text.plain
    assert "idle" not in text.plain
