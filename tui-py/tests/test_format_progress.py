"""Tests for the byte and progress-bar formatters shared by the scan bar and
the picker's analysis line."""

from __future__ import annotations

from format_utils import BAR_EMPTY, BAR_FILLED, format_bytes, progress_bar


def test_format_bytes_matches_the_web_units():
    assert format_bytes(512) == "512 B"
    assert format_bytes(1_500) == "1.5 KB"
    assert format_bytes(2_400_000) == "2.4 MB"
    assert format_bytes(9_100_000_000) == "9.1 GB"


def test_progress_bar_is_fixed_width():
    for percent in (0, 1, 37, 99, 100):
        assert len(progress_bar(percent, 10)) == 10


def test_progress_bar_fills_proportionally():
    assert progress_bar(0, 10) == BAR_EMPTY * 10
    assert progress_bar(50, 10) == BAR_FILLED * 5 + BAR_EMPTY * 5
    assert progress_bar(100, 10) == BAR_FILLED * 10


def test_progress_bar_clamps_out_of_range_percentages():
    """A backend reporting 120 (or a negative) must not draw a wider bar."""
    assert progress_bar(120, 10) == BAR_FILLED * 10
    assert progress_bar(-5, 10) == BAR_EMPTY * 10
