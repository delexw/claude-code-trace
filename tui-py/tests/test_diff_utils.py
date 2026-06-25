"""Tests for diff_utils — mirror of shared/diff.test.ts."""

from __future__ import annotations

from diff_utils import compute_edit_diff, tokenize, word_diff


def test_tokenize_splits_words_whitespace_punctuation():
    assert tokenize("const x = 1;") == ["const", " ", "x", " ", "=", " ", "1", ";"]


def test_tokenize_empty():
    assert tokenize("") == []


def test_tokenize_keeps_underscore_in_word():
    assert tokenize("foo_bar baz") == ["foo_bar", " ", "baz"]


def test_word_diff_highlights_only_changed_token():
    wd = word_diff("const x = 1;", "const x = 2;")
    assert wd is not None
    old_segs, new_segs = wd
    assert [(s.text, s.changed) for s in old_segs] == [
        ("const x = ", False),
        ("1", True),
        (";", False),
    ]
    assert [(s.text, s.changed) for s in new_segs] == [
        ("const x = ", False),
        ("2", True),
        (";", False),
    ]


def test_word_diff_returns_none_when_too_dissimilar():
    assert word_diff("import foo from 'a';", "xyz();") is None


def test_word_diff_returns_none_for_blank_lines():
    assert word_diff("", "") is None
    assert word_diff("   ", "  ") is None


def test_compute_edit_diff_preserves_context():
    diff = compute_edit_diff(["a", "foo bar", "c"], ["a", "foo baz", "c"])
    assert [d.kind for d in diff] == ["context", "removed", "added", "context"]
    assert [(s.text, s.changed) for s in diff[0].segments] == [("a", False)]
    assert [(s.text, s.changed) for s in diff[3].segments] == [("c", False)]


def test_compute_edit_diff_word_highlights_pair():
    diff = compute_edit_diff(["foo bar"], ["foo baz"])
    removed = next(d for d in diff if d.kind == "removed")
    added = next(d for d in diff if d.kind == "added")
    assert [(s.text, s.changed) for s in removed.segments] == [
        ("foo ", False),
        ("bar", True),
    ]
    assert [(s.text, s.changed) for s in added.segments] == [
        ("foo ", False),
        ("baz", True),
    ]


def test_compute_edit_diff_pure_insertion():
    diff = compute_edit_diff(["const x = 1;"], ["const x = 2;", "const y = 3;"])
    assert len([d for d in diff if d.kind == "removed"]) == 1
    assert len([d for d in diff if d.kind == "added"]) == 2
    unpaired = [d for d in diff if d.kind == "added"][1]
    assert [(s.text, s.changed) for s in unpaired.segments] == [("const y = 3;", False)]


def test_compute_edit_diff_dissimilar_lines_not_word_split():
    diff = compute_edit_diff(["import foo from 'a';"], ["xyz();"])
    removed = next(d for d in diff if d.kind == "removed")
    added = next(d for d in diff if d.kind == "added")
    assert [(s.text, s.changed) for s in removed.segments] == [("import foo from 'a';", False)]
    assert [(s.text, s.changed) for s in added.segments] == [("xyz();", False)]


def test_compute_edit_diff_identical_is_all_context():
    diff = compute_edit_diff(["same"], ["same"])
    assert len(diff) == 1
    assert diff[0].kind == "context"


def test_compute_edit_diff_reconstructs_line_from_segments():
    diff = compute_edit_diff(
        ["function f(a) {", "  return a + 1;", "}"],
        ["function f(a) {", "  return a + 2;", "}"],
    )
    changed = next(d for d in diff if d.kind == "removed")
    assert "".join(s.text for s in changed.segments) == "  return a + 1;"
