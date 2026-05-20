"""Shared ListView base used by every list page (SessionPicker, MessageList,
DetailView items). One class, one CSS rule, one initial-highlight policy.

Why this exists: Textual's default ListView highlight has two problems for us:

1. The class name `.-highlight` is set on the cursor row, but Textual's
   default rule lights it up with a bright `$block-cursor-background` when
   focused — much louder than the dim `$block-cursor-blurred-background`
   used when blurred. We want the calmer dim color in *both* states.

2. After `clear()` + `append()`, ListView leaves `index = None`, so on
   first render no row is highlighted. Every page worked around this
   separately; now it lives here.

Subclasses can override `_first_selectable_index()` to skip disabled
header rows (e.g. SessionPicker's "Sessions (N)" + date group headers).
"""

from __future__ import annotations

import contextlib

from textual.widgets import ListView

from keybindings import BASE_BINDINGS


class HighlightListView(ListView):
    """ListView with the project's shared highlight styling + auto-index."""

    BINDINGS = [*BASE_BINDINGS]

    DEFAULT_CSS = """
    HighlightListView:focus {
        background-tint: transparent;
    }
    HighlightListView > ListItem.-highlight,
    HighlightListView:focus > ListItem.-highlight {
        background: $block-cursor-blurred-background;
        color: $block-cursor-blurred-foreground;
        text-style: $block-cursor-blurred-text-style;
    }
    """

    def _first_selectable_index(self) -> int | None:
        """Return the first index whose ListItem is not disabled, or None."""
        for idx, node in enumerate(self._nodes):
            if not getattr(node, "disabled", False):
                return idx
        return None

    def ensure_highlight(self) -> None:
        """Move the cursor to the first selectable row if none is set.

        Call this after a clear() + append() cycle. Idempotent — does
        nothing if the user already has a row selected.
        """
        with contextlib.suppress(Exception):
            if self.index is None:
                first = self._first_selectable_index()
                if first is not None:
                    self.index = first
