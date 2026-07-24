"""Tests for project-tree node labelling (Python port of shared/projectTree.ts)."""

from __future__ import annotations

from data_types import SessionInfo
from project_tree import build_project_nodes


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
