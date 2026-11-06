"""Codex's folder tree reports selection to its owning plugin."""

from collections.abc import Awaitable, Callable

from widgets.project_tree import ProjectTree


class CodexProjectTree(ProjectTree):
    def __init__(
        self,
        on_select: Callable[[str | None], None],
        on_poll: Callable[[], Awaitable[None]],
        **kwargs,
    ):
        super().__init__(heading="Codex Projects", **kwargs)
        self._on_select = on_select
        self._on_poll = on_poll

    def on_mount(self) -> None:
        super().on_mount()
        self.set_interval(2, self._on_poll)

    def on_project_tree_project_selected(self, event: ProjectTree.ProjectSelected) -> None:
        event.stop()
        self._on_select(event.project_key)
