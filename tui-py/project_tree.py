"""Project tree building logic — Python port of shared/projectTree.ts."""

from __future__ import annotations

from dataclasses import dataclass, field

from data_types import FlatItem, SessionInfo
from format_utils import project_display_name, project_key, short_path

# ---------------------------------------------------------------------------
# ProjectNode
# ---------------------------------------------------------------------------


@dataclass
class ProjectNode:
    name: str
    key: str
    session_count: int
    has_ongoing: bool


def build_project_nodes(sessions: list[SessionInfo]) -> list[ProjectNode]:
    mapping: dict[str, ProjectNode] = {}
    for s in sessions:
        key = project_key(s.path)
        if key in mapping:
            mapping[key].session_count += 1
            if s.is_ongoing:
                mapping[key].has_ongoing = True
        else:
            name = short_path(s.cwd) or project_display_name(key)
            mapping[key] = ProjectNode(
                name=name,
                key=key,
                session_count=1,
                has_ongoing=s.is_ongoing,
            )
    return sorted(mapping.values(), key=lambda n: n.name)


# ---------------------------------------------------------------------------
# TreeNode
# ---------------------------------------------------------------------------


@dataclass
class TreeNode:
    node: ProjectNode
    children: list[TreeNode] = field(default_factory=list)


def build_tree(nodes: list[ProjectNode]) -> list[TreeNode]:
    sorted_nodes = sorted(nodes, key=lambda n: len(n.key))
    roots: list[TreeNode] = []
    all_nodes: list[TreeNode] = []

    for node in sorted_nodes:
        parent: TreeNode | None = None
        for candidate in all_nodes:
            if node.key.startswith(candidate.node.key + "-") and (
                parent is None or len(candidate.node.key) > len(parent.node.key)
            ):
                parent = candidate

        tn = TreeNode(node=node)
        all_nodes.append(tn)

        if parent is not None:
            suffix = node.key[len(parent.node.key) :]
            suffix_parts = suffix.split("--")
            intermediates = [p.lstrip("-") for p in suffix_parts[:-1] if p.lstrip("-")]

            if intermediates:
                attach_to = parent
                for seg in intermediates:
                    virtual_key = f"__virtual:{attach_to.node.key}:{seg}"
                    vn = next(
                        (c for c in attach_to.children if c.node.key == virtual_key),
                        None,
                    )
                    if vn is None:
                        vn = TreeNode(
                            node=ProjectNode(
                                name=seg,
                                key=virtual_key,
                                session_count=0,
                                has_ongoing=False,
                            )
                        )
                        attach_to.children.append(vn)
                        all_nodes.append(vn)
                    vn.node.session_count += node.session_count
                    vn.node.has_ongoing = vn.node.has_ongoing or node.has_ongoing
                    attach_to = vn
                attach_to.children.append(tn)
            else:
                child_label = node.key[len(parent.node.key) :].lstrip("-") or project_display_name(
                    node.key
                )
                tn.node = ProjectNode(
                    name=node.name or child_label,
                    key=node.key,
                    session_count=node.session_count,
                    has_ongoing=node.has_ongoing,
                )
                parent.children.append(tn)
        else:
            roots.append(tn)

    roots.sort(key=lambda t: t.node.name)
    for r in all_nodes:
        r.children.sort(key=lambda t: t.node.name)

    return roots


# ---------------------------------------------------------------------------
# Worktree helpers
# ---------------------------------------------------------------------------

WorktreeKind = str  # "worktrees" | "claude-worktrees"


def detect_worktree_kind(parent_key: str, child_key: str) -> WorktreeKind | None:
    parts = child_key.split("--")
    if len(parts) > 1:
        last = parts[-1]
        if last.startswith("claude-worktrees"):
            return "claude-worktrees"
        if last.startswith("worktrees"):
            return "worktrees"
        return None
    rest = child_key[len(parent_key) :]
    if rest.startswith("--claude-worktrees-"):
        return "claude-worktrees"
    if rest.startswith("-worktrees-"):
        return "worktrees"
    return None


def worktree_leaf_name(parent_key: str, child_key: str, kind: WorktreeKind) -> str:
    parts = child_key.split("--")
    if len(parts) > 1:
        last = parts[-1]
        prefix = "claude-worktrees-" if kind == "claude-worktrees" else "worktrees-"
        if last.startswith(prefix):
            return last[len(prefix) :]
        return project_display_name(child_key)
    prefix_len = len("--claude-worktrees-") if kind == "claude-worktrees" else len("-worktrees-")
    return child_key[len(parent_key) + prefix_len :].lstrip("-") or project_display_name(child_key)


# ---------------------------------------------------------------------------
# FlatItem flattening
# ---------------------------------------------------------------------------


def flatten_tree(roots: list[TreeNode], collapsed_keys: set[str] | None = None) -> list[FlatItem]:
    items: list[FlatItem] = []
    if collapsed_keys is None:
        collapsed_keys = set()

    def walk(nodes: list[TreeNode], depth: int) -> None:
        for tn in nodes:
            is_collapsed = tn.node.key in collapsed_keys
            has_children = len(tn.children) > 0

            items.append(
                FlatItem(
                    key=tn.node.key,
                    name=tn.node.name,
                    count=tn.node.session_count,
                    ongoing=tn.node.has_ongoing,
                    depth=depth,
                    is_group=False,
                    has_children=has_children,
                    is_expanded=not is_collapsed,
                )
            )

            if is_collapsed:
                continue

            # Categorise children
            groups: dict[str, list[TreeNode]] = {}
            regular: list[TreeNode] = []
            for child in tn.children:
                kind = detect_worktree_kind(tn.node.key, child.node.key)
                if kind:
                    groups.setdefault(kind, []).append(child)
                else:
                    regular.append(child)

            for kind, children in groups.items():
                total_count = sum(c.node.session_count for c in children)
                any_ongoing = any(c.node.has_ongoing for c in children)
                group_key = f"__group:{kind}:{tn.node.key}"
                group_collapsed = group_key in collapsed_keys

                items.append(
                    FlatItem(
                        key=group_key,
                        name=kind,
                        count=total_count,
                        ongoing=any_ongoing,
                        depth=depth + 1,
                        is_group=True,
                        has_children=len(children) > 0,
                        is_expanded=not group_collapsed,
                    )
                )

                if not group_collapsed:
                    for child in children:
                        child_collapsed = child.node.key in collapsed_keys
                        items.append(
                            FlatItem(
                                key=child.node.key,
                                name=worktree_leaf_name(tn.node.key, child.node.key, kind),
                                count=child.node.session_count,
                                ongoing=child.node.has_ongoing,
                                depth=depth + 2,
                                is_group=False,
                                has_children=len(child.children) > 0,
                                is_expanded=not child_collapsed,
                            )
                        )
                        if not child_collapsed:
                            walk(child.children, depth + 3)

            walk(regular, depth + 1)

    walk(roots, 0)
    return items


def build_flat_items(
    sessions: list[SessionInfo], collapsed_keys: set[str] | None = None
) -> list[FlatItem]:
    """Build the complete flat tree including 'All Projects' header."""
    nodes = build_project_nodes(sessions)
    tree = build_tree(nodes)
    flat = flatten_tree(tree, collapsed_keys)
    return [
        FlatItem(
            key=None,
            name="All Projects",
            count=len(sessions),
            ongoing=False,
            depth=0,
            is_group=False,
            has_children=False,
            is_expanded=True,
        ),
        *flat,
    ]
