"""SourcePicker — the shared session list, built from every enabled plugin's rows."""

from __future__ import annotations

from collections.abc import Iterable

from textual.widgets import ListItem, Static

import theme
from plugins.base import PickerRow, PickerSection
from widgets.highlight_list import HighlightListView


class SourcePicker(HighlightListView):
    """One list for all sources: each plugin's section is appended in order.

    Header rows are disabled. Rows with an item map back to
    ``(plugin_id, item)`` through ``entry_at_raw_index``.
    """

    DEFAULT_CSS = """
    SourcePicker {
        height: 100%;
    }
    SourcePicker ListItem {
        padding: 1 2;
    }
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._entries: dict[int, tuple[str, PickerRow]] = {}
        self._anim_frame: int = 0

    def on_mount(self) -> None:
        self.set_interval(0.5, self._spin)

    def _spin(self) -> None:
        """Advance the spinner and re-render the rows that show live activity."""
        try:
            animated = [
                (raw_idx, row) for raw_idx, (_pid, row) in self._entries.items() if row.animated
            ]
            if not animated:
                return
            self._anim_frame = (self._anim_frame + 1) % len(theme.SPIN)
            for raw_idx, row in animated:
                self._refresh_item(raw_idx, row)
        except Exception:
            pass

    def _refresh_item(self, raw_idx: int, row: PickerRow) -> None:
        try:
            item = self._nodes[raw_idx]
            item.query_one(Static).update(row.render(self._anim_frame))
        except Exception:
            pass

    # ----------------------------------------------------------------
    # Public API
    # ----------------------------------------------------------------

    def populate(self, sections: list[tuple[str, PickerSection]]) -> None:
        """Rebuild the list from ``(plugin_id, section)`` pairs."""
        self._entries = {}

        # Eagerly clear so neither stale rows nor the previous overlay sit
        # under the new state.
        self.clear()
        self.loading = any(section.loading for _pid, section in sections)

        if self.loading:
            return

        raw_idx = 0
        for plugin_id, section in sections:
            for row in section.rows:
                self.append(ListItem(Static(row.render(self._anim_frame)), disabled=row.header))
                if row.item is not None:
                    self._entries[raw_idx] = (plugin_id, row)
                raw_idx += 1

        # Pick the first selectable row (HighlightListView skips disabled headers).
        self.ensure_highlight()

    def refresh_rows(self, plugin_id: str, items: Iterable[object]) -> None:
        """Re-render one plugin's rows for ``items`` without rebuilding the list,
        so the cursor stays put."""
        wanted = list(items)
        for raw_idx, (pid, row) in self._entries.items():
            if pid == plugin_id and any(row.item is item for item in wanted):
                self._refresh_item(raw_idx, row)

    def entry_at_raw_index(self, raw_idx: int) -> tuple[str, object] | None:
        """``(plugin_id, item)`` at a raw ListView node index; None for a header."""
        entry = self._entries.get(raw_idx)
        if entry is None:
            return None
        plugin_id, row = entry
        return plugin_id, row.item

    def selectable_indices(self) -> list[int]:
        return sorted(self._entries)
