"""Tests for project-tree node labelling (Python port of shared/projectTree.ts)."""

from __future__ import annotations

from data_types import SessionInfo
from project_tree import build_project_nodes, resolve_fork_root


def test_labels_roaming_session_by_origin_dir_not_last_cwd():
    # A session that started in sso-server but /cd'd into other repos: the file
    # still lives under the sso-server folder, so the node must be named for the
    # origin (dirs[0]), not the last-seen cwd (dirs[-1] == cwd).
    sessions = [
        SessionInfo(
            path="/home/user/.claude/projects/-Users-me-repos-sso-server/s1.jsonl",
            dirs=["/Users/me/repos/sso-server", "/Users/me/seo/elements-backend"],
            cwd="/Users/me/seo/elements-backend",
        )
    ]
    nodes = build_project_nodes(sessions)
    assert len(nodes) == 1
    assert nodes[0].name == "sso-server"


def test_falls_back_to_cwd_when_dirs_empty():
    sessions = [
        SessionInfo(
            path="/home/user/.claude/projects/proj-a/s1.jsonl",
            cwd="/home/user/proj-a",
            dirs=[],
        )
    ]
    assert build_project_nodes(sessions)[0].name == "proj-a"


# --- Issue #238: /fork's own worktree (v2.1.221+) — group forked sessions by fork parent ---


def test_forked_session_groups_under_fork_parent_project():
    # The forked session's own cwd is a brand-new, unrelated worktree — it must still
    # be counted under the parent project's node, not create its own.
    parent = SessionInfo(
        path="/home/user/.claude/projects/-Users-me-repos-my-app/parent.jsonl",
        session_id="parent",
        dirs=["/Users/me/repos/my-app"],
        cwd="/Users/me/repos/my-app",
    )
    forked = SessionInfo(
        path="/home/user/.claude/projects/-Users-me-worktrees-abc123/forked.jsonl",
        session_id="forked",
        dirs=["/Users/me/worktrees/abc123"],
        cwd="/Users/me/worktrees/abc123",
        forked_from_session_id="parent",
    )
    nodes = build_project_nodes([parent, forked])
    assert len(nodes) == 1
    assert nodes[0].name == "my-app"
    assert nodes[0].session_count == 2


def test_forked_session_falls_back_to_own_project_when_parent_missing():
    # Parent session isn't in the current listing (e.g. its file was deleted) — the
    # forked session must fall back to grouping under its own project, not be dropped.
    forked = SessionInfo(
        path="/home/user/.claude/projects/-Users-me-worktrees-abc123/forked.jsonl",
        session_id="forked",
        dirs=["/Users/me/worktrees/abc123"],
        cwd="/Users/me/worktrees/abc123",
        forked_from_session_id="missing-parent",
    )
    nodes = build_project_nodes([forked])
    assert len(nodes) == 1
    assert nodes[0].name == "abc123"


def test_resolve_fork_root_follows_multi_hop_chain():
    # A fork of a fork must resolve all the way back to the ultimate ancestor.
    grandparent = SessionInfo(session_id="gp")
    parent = SessionInfo(session_id="p", forked_from_session_id="gp")
    child = SessionInfo(session_id="c", forked_from_session_id="p")
    by_id = {s.session_id: s for s in [grandparent, parent, child]}
    assert resolve_fork_root(child, by_id).session_id == "gp"


def test_resolve_fork_root_guards_against_cycles():
    a = SessionInfo(session_id="a", forked_from_session_id="b")
    b = SessionInfo(session_id="b", forked_from_session_id="a")
    by_id = {s.session_id: s for s in [a, b]}
    # Must terminate rather than looping forever; exact landing session isn't load-bearing.
    result = resolve_fork_root(a, by_id)
    assert result.session_id in ("a", "b")
