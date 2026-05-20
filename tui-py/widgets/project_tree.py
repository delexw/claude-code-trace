"""ProjectTree sidebar widget — uses Textual's Tree widget."""

from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.message import Message
from textual.widget import Widget
from textual.widgets import Tree
from textual.widgets.tree import TreeNode

import theme
from data_types import FlatItem, SessionInfo
from project_tree import build_flat_items


class _VimTree(Tree):
    """Tree subclass that adds j/k Vim navigation."""

    BINDINGS = [
        Binding("j", "cursor_down", "↓", show=True),
        Binding("k", "cursor_up", "↑", show=True),
    ]


class ProjectTree(Widget):
    """Sidebar showing project hierarchy with expand/collapse.

    Uses Textual's built-in Tree widget for navigation and selection.
    Posts a ProjectSelected message when the user picks a project.
    """

    DEFAULT_CSS = """
    ProjectTree {
        border: round $border;
        width: 30;
        min-width: 28;
        max-width: 40%;
    }
    ProjectTree Tree {
        background: transparent;
    }
    """

    class ProjectSelected(Message):
        """Fired when the user selects a project node."""

        def __init__(self, project_key: str | None) -> None:
            super().__init__()
            self.project_key = project_key

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._sessions: list[SessionInfo] = []
        self._selected_project: str | None = None
        self._collapsed_keys: set[str] = set()
        self._anim_frame: int = 0
        self._ongoing_nodes: list[tuple[TreeNode[FlatItem], FlatItem]] = []

    def compose(self) -> ComposeResult:
        tree: _VimTree = _VimTree("Projects", id="tree-inner")
        tree.show_root = False
        tree.guide_depth = 2
        yield tree

    def on_mount(self) -> None:
        self._rebuild()
        self.set_interval(0.5, self._spin)

    def _spin(self) -> None:
        """Advance spinner frame and update only ongoing node labels in place."""
        try:
            if not self._ongoing_nodes:
                return
            self._anim_frame = (self._anim_frame + 1) % len(theme.SPIN)
            for node, item in self._ongoing_nodes:
                node.set_label(self._format_label(item))
        except Exception:
            pass

    # ----------------------------------------------------------------
    # Public API for the App to push new state
    # ----------------------------------------------------------------

    def update_state(
        self,
        sessions: list[SessionInfo],
        selected_project: str | None,
        collapsed_keys: set[str],
    ) -> None:
        changed = (
            sessions != self._sessions
            or selected_project != self._selected_project
            or collapsed_keys != self._collapsed_keys
        )
        self._sessions = sessions
        self._selected_project = selected_project
        self._collapsed_keys = collapsed_keys
        if changed:
            self._rebuild()

    # ----------------------------------------------------------------
    # Internal rebuild
    # ----------------------------------------------------------------

    def _rebuild(self) -> None:
        try:
            tree = self.query_one("#tree-inner", _VimTree)
        except Exception:
            return

        tree.clear()
        self._ongoing_nodes = []
        entries = build_flat_items(self._sessions, self._collapsed_keys)

        # Build a flat list of nodes; track parent stack by depth
        parent_stack: list[TreeNode[FlatItem]] = []

        for item in entries:
            # Find correct parent
            while len(parent_stack) > item.depth:
                parent_stack.pop()

            label = self._format_label(item)
            parent = parent_stack[-1] if parent_stack else tree.root

            if item.has_children:
                node = parent.add(label, data=item, expand=item.is_expanded)
            else:
                node = parent.add_leaf(label, data=item)

            if item.ongoing:
                self._ongoing_nodes.append((node, item))

            parent_stack.append(node)

        # Highlight the currently selected project
        self._highlight_selected(tree)

    def _format_label(self, item: FlatItem) -> str:
        """Build the Rich-markup label text for a tree node."""
        spin = theme.SPIN[self._anim_frame] if item.ongoing else ""
        count_str = f" ({item.count})"

        if item.key is None:
            is_selected = self._selected_project is None
            if is_selected:
                return f"[bold {theme.ACCENT}]★ All Projects{count_str}[/]"
            return f"[{theme.TEXT_DIM}]All Projects{count_str}[/]"

        if item.is_group:
            return f"[{theme.TEXT_DIM}]≡ {item.name}{count_str}[/]"

        is_selected = item.key == self._selected_project
        if is_selected:
            suffix = f" [{theme.ONGOING}]{spin}[/]" if item.ongoing else ""
            return f"[bold {theme.ACCENT}]▸ {item.name}{count_str}[/]{suffix}"
        if item.ongoing:
            return f"{item.name}{count_str} [{theme.ONGOING}]{spin}[/]"
        return f"{item.name}[{theme.TEXT_DIM}]{count_str}[/]"

    def _highlight_selected(self, tree: _VimTree) -> None:
        """Walk nodes and ensure the selected one is visible."""

        def walk(node: TreeNode[FlatItem]) -> bool:
            data: FlatItem | None = node.data
            if data is not None:
                is_selected = data.key == self._selected_project or (
                    data.key is None and self._selected_project is None
                )
                if is_selected and not data.is_group:
                    tree.select_node(node)
                    return True
            return any(walk(child) for child in node.children)

        walk(tree.root)

    # ----------------------------------------------------------------
    # React to Tree selection events and focus events
    # ----------------------------------------------------------------

    def on_tree_node_selected(self, event: Tree.NodeSelected) -> None:
        event.stop()
        data: FlatItem | None = event.node.data
        if data is None or data.is_group:
            return
        self._selected_project = data.key
        self.post_message(self.ProjectSelected(data.key))

    def on_key(self, event) -> None:
        """Allow l/right/escape to move focus back to the content area."""
        if event.key in ("l", "right", "escape"):
            event.stop()
            # Post a no-change project selection to trigger focus move in App
            self.post_message(self.ProjectSelected(self._selected_project))
