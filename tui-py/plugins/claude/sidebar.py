"""Claude's sidebar: the project tree, which filters Claude's picker rows."""

from __future__ import annotations

from collections.abc import Callable

from widgets.project_tree import ProjectTree


class ClaudeProjectTree(ProjectTree):
    """The shared ProjectTree, reporting selections to the Claude plugin."""

    def __init__(self, on_select: Callable[[str | None], None], **kwargs):
        super().__init__(**kwargs)
        self._on_select = on_select

    def on_project_tree_project_selected(self, event: ProjectTree.ProjectSelected) -> None:
        event.stop()
        self._on_select(event.project_key)
