"""Structural diff for an Edit tool's old_string -> new_string.

Produces a line-level unified diff that PRESERVES unchanged context lines, and
for each pair of changed lines computes intra-line WORD-level change ranges.
Inspired by umputun/revdiff's worddiff: LCS pairing + a similarity gate so
dissimilar lines aren't falsely aligned.

Mirror of shared/diff.ts — keep the two implementations in sync.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

# Beyond this many DP cells the O(n*m) line LCS is skipped in favour of a plain
# "all removed then all added" rendering. Edit payloads are small in practice.
MAX_LCS_CELLS = 40000

# Minimum fraction of shared non-whitespace tokens for two lines to be treated
# as an edit of each other (and thus word-diffed). Matches revdiff's 30% gate.
WORD_SIMILARITY_THRESHOLD = 0.3

# Words (letters/digits/underscore), whitespace runs, and punctuation runs.
_TOKEN_RE = re.compile(r"\w+|\s+|[^\w\s]+", re.UNICODE)


@dataclass
class DiffSegment:
    text: str
    changed: bool


@dataclass
class DiffLine:
    kind: str  # "context" | "removed" | "added"
    segments: list[DiffSegment]


def tokenize(s: str) -> list[str]:
    return _TOKEN_RE.findall(s)


def _is_ws(tok: str) -> bool:
    return tok.strip() == ""


def _lcs_matched(a: list[str], b: list[str]) -> tuple[list[bool], list[bool]]:
    """Mark which elements of a / b participate in their longest common subsequence."""
    n, m = len(a), len(b)
    a_matched = [False] * n
    b_matched = [False] * m
    if n == 0 or m == 0:
        return a_matched, b_matched

    dp = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n - 1, -1, -1):
        for j in range(m - 1, -1, -1):
            if a[i] == b[j]:
                dp[i][j] = dp[i + 1][j + 1] + 1
            else:
                dp[i][j] = max(dp[i + 1][j], dp[i][j + 1])

    i = j = 0
    while i < n and j < m:
        if a[i] == b[j]:
            a_matched[i] = True
            b_matched[j] = True
            i += 1
            j += 1
        elif dp[i + 1][j] >= dp[i][j + 1]:
            i += 1
        else:
            j += 1
    return a_matched, b_matched


def _build_segments(tokens: list[str], matched: list[bool]) -> list[DiffSegment]:
    """Merge adjacent tokens with the same changed flag; whitespace never flags changed."""
    segs: list[DiffSegment] = []
    for i, tok in enumerate(tokens):
        changed = (not matched[i]) and (not _is_ws(tok))
        if segs and segs[-1].changed == changed:
            segs[-1].text += tok
        else:
            segs.append(DiffSegment(tok, changed))
    return segs


def word_diff(old_line: str, new_line: str) -> tuple[list[DiffSegment], list[DiffSegment]] | None:
    """Word-level diff of two lines, or None when too dissimilar to align."""
    a = tokenize(old_line)
    b = tokenize(new_line)
    a_matched, b_matched = _lcs_matched(a, b)

    a_non_ws = sum(1 for t in a if not _is_ws(t))
    b_non_ws = sum(1 for t in b if not _is_ws(t))
    denom = max(a_non_ws, b_non_ws)
    if denom == 0:
        return None

    common = sum(1 for i, t in enumerate(a) if a_matched[i] and not _is_ws(t))
    if common / denom < WORD_SIMILARITY_THRESHOLD:
        return None

    return _build_segments(a, a_matched), _build_segments(b, b_matched)


def _line_diff_ops(old_lines: list[str], new_lines: list[str]) -> list[tuple[str, str]]:
    n, m = len(old_lines), len(new_lines)
    if n * m > MAX_LCS_CELLS:
        return [("removed", t) for t in old_lines] + [("added", t) for t in new_lines]

    dp = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(n - 1, -1, -1):
        for j in range(m - 1, -1, -1):
            if old_lines[i] == new_lines[j]:
                dp[i][j] = dp[i + 1][j + 1] + 1
            else:
                dp[i][j] = max(dp[i + 1][j], dp[i][j + 1])

    ops: list[tuple[str, str]] = []
    i = j = 0
    while i < n and j < m:
        if old_lines[i] == new_lines[j]:
            ops.append(("context", old_lines[i]))
            i += 1
            j += 1
        elif dp[i + 1][j] >= dp[i][j + 1]:
            ops.append(("removed", old_lines[i]))
            i += 1
        else:
            ops.append(("added", new_lines[j]))
            j += 1
    while i < n:
        ops.append(("removed", old_lines[i]))
        i += 1
    while j < m:
        ops.append(("added", new_lines[j]))
        j += 1
    return ops


def _group_runs(ops: list[tuple[str, str]]) -> list[tuple[str, str]]:
    """Within each maximal run of changes, emit removed lines before added lines."""
    out: list[tuple[str, str]] = []
    k = 0
    n = len(ops)
    while k < n:
        if ops[k][0] == "context":
            out.append(ops[k])
            k += 1
            continue
        removed: list[tuple[str, str]] = []
        added: list[tuple[str, str]] = []
        while k < n and ops[k][0] != "context":
            if ops[k][0] == "removed":
                removed.append(ops[k])
            else:
                added.append(ops[k])
            k += 1
        out.extend(removed)
        out.extend(added)
    return out


def compute_edit_diff(old_lines: list[str], new_lines: list[str]) -> list[DiffLine]:
    ops = _group_runs(_line_diff_ops(old_lines, new_lines))
    result: list[DiffLine] = []
    k = 0
    n = len(ops)
    while k < n:
        if ops[k][0] == "context":
            result.append(DiffLine("context", [DiffSegment(ops[k][1], False)]))
            k += 1
            continue

        removed: list[str] = []
        added: list[str] = []
        while k < n and ops[k][0] == "removed":
            removed.append(ops[k][1])
            k += 1
        while k < n and ops[k][0] == "added":
            added.append(ops[k][1])
            k += 1

        pairs = min(len(removed), len(added))
        wds = [word_diff(removed[i], added[i]) for i in range(pairs)]

        for i, text in enumerate(removed):
            wd = wds[i] if i < pairs else None
            segs = wd[0] if wd else [DiffSegment(text, False)]
            result.append(DiffLine("removed", segs))
        for i, text in enumerate(added):
            wd = wds[i] if i < pairs else None
            segs = wd[1] if wd else [DiffSegment(text, False)]
            result.append(DiffLine("added", segs))
    return result
