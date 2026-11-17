"""Source metadata renders accurate context limits and estimated/unknown costs."""

import pytest

from data_types import SessionMeta, SessionTotals
from widgets.info_bar import InfoBar


@pytest.mark.parametrize(
    "cost, expected",
    [
        (None, "Cost unavailable"),
        (0.0, "~$0.00 estimated API cost"),
        (1.25, "~$1.25 estimated API cost"),
    ],
)
def test_source_cost_is_explicit_about_unknown_zero_and_estimated_values(cost, expected):
    bar = InfoBar()
    bar.session_totals = SessionTotals(cost_usd=cost, estimated_cost=True)
    assert expected in bar._build_content()


def test_source_context_uses_reported_limit_and_real_session_id():
    bar = InfoBar()
    bar.session_path = "/example/.codex/sessions/rollout-fabricated.jsonl"
    bar.meta = SessionMeta(
        session_id="example-thread", cwd="/example/app", context_tokens=25000, context_window=100000
    )
    content = bar._build_content()
    assert "ctx 25%" in content
    assert "example-" in content
    assert "rollout-" not in content
    bar.meta = SessionMeta(context_tokens=25000, context_window=0)
    assert "ctx " not in bar._build_content()


def test_partial_estimate_marks_the_reported_subtotal():
    bar = InfoBar()
    bar.session_totals = SessionTotals(cost_usd=1.25, estimated_cost=True, cost_partial=True)
    assert "~$1.25+ estimated API cost" in bar._build_content()
