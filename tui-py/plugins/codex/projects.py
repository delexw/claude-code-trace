"""Codex project identity and folder hierarchy, based on real working directories."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field

from data_types import FlatItem
from plugins.codex.models import CodexSession


def _clean_path(path: str) -> str:
    root = "//" if path.startswith("//") else "/" if path.startswith("/") else ""
    parts: list[str] = []
    for part in path.split("/"):
        if not part or part == ".":
            continue
        if part == ".." and parts and parts[-1] != "..":
            parts.pop()
        elif part != ".." or not root:
            parts.append(part)
    return root + "/".join(parts)


def _folder(session: CodexSession) -> tuple[str, str, str, str]:
    dirs = getattr(session, "dirs", [])
    original = (dirs[0] if dirs else session.cwd).replace("\\", "/")
    unc = re.match(r"^//(?:wsl\.localhost|wsl\$)/([^/]+)(/.*)?$", original, re.I)
    transcript = re.match(
        r"^//(?:wsl\.localhost|wsl\$)/([^/]+)", session.path.replace("\\", "/"), re.I
    )
    source = re.match(r"^WSL:\s*(.+)$", session.source, re.I)
    distro = unc[1] if unc else transcript[1] if transcript else source[1] if source else ""
    windows = not unc and bool(re.match(r"^(?:[a-z]:|//)", original, re.I))
    scope = "windows" if windows else f"wsl:{distro.lower()}" if distro else "posix"
    display_path = _clean_path((unc[2] or "/") if unc else original)
    path = display_path.lower() if windows else display_path
    key = json.dumps([scope, path], separators=(",", ":"), ensure_ascii=False)
    label = display_path.rstrip("/").rsplit("/", 1)[-1] or display_path or "Unknown folder"
    return key, scope, path, label


def codex_project_key(session: CodexSession) -> str:
    return _folder(session)[0]


def _fork_root(session: CodexSession, by_id: dict[str, CodexSession]) -> CodexSession:
    current = session
    seen: set[str] = set()
    while (
        parent_id := getattr(current, "forked_from_session_id", None)
    ) and current.id not in seen:
        seen.add(current.id)
        parent = by_id.get(parent_id)
        if parent is None:
            break
        current = parent
    return current


def filter_project_sessions(
    sessions: list[CodexSession], selected_project: str | None
) -> list[CodexSession]:
    if selected_project is None:
        return sessions
    by_id = {session.id: session for session in sessions}
    return [
        session
        for session in sessions
        if codex_project_key(_fork_root(session, by_id)) == selected_project
    ]


@dataclass
class _FolderNode:
    key: str
    scope: str
    path: str
    label: str
    count: int = 0
    ongoing: bool = False
    children: list[_FolderNode] = field(default_factory=list)


def build_project_items(sessions: list[CodexSession]) -> list[FlatItem]:
    by_id = {session.id: session for session in sessions}
    folders: dict[str, _FolderNode] = {}
    for session in sessions:
        key, scope, path, label = _folder(_fork_root(session, by_id))
        node = folders.setdefault(key, _FolderNode(key, scope, path, label))
        node.count += 1
        node.ongoing |= bool(getattr(session, "is_ongoing", False))
    roots: list[_FolderNode] = []
    for node in folders.values():
        ancestors = [
            parent
            for parent in folders.values()
            if node.scope == parent.scope
            and node.path != parent.path
            and parent.path
            and node.path.startswith(parent.path.rstrip("/") + "/")
        ]
        parent = max(ancestors, key=lambda node: len(node.path)) if ancestors else None
        (parent.children if parent else roots).append(node)
    items = [FlatItem(name="All Projects", count=len(sessions))]

    def walk(children: list[_FolderNode], depth: int) -> None:
        for node in sorted(children, key=lambda node: (node.label.casefold(), node.key)):
            items.append(
                FlatItem(
                    key=node.key,
                    name=node.label,
                    count=node.count,
                    ongoing=node.ongoing,
                    depth=depth,
                    has_children=bool(node.children),
                )
            )
            walk(node.children, depth + 1)

    walk(roots, 0)
    return items
