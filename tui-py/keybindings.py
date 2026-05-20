"""Shared binding constants reused across all view widgets.

Usage:
    from keybindings import BASE_BINDINGS
    class MyWidget(Widget):
        BINDINGS = [*BASE_BINDINGS, Binding("x", "do_x", "X")]
"""

from textual.binding import Binding

# Shown on every page — j/k navigation and Enter confirm.
# All list-based widgets (ListView subclasses) already implement cursor_down /
# cursor_up; DetailView defines its own action_cursor_down / action_cursor_up.
BASE_BINDINGS: list[Binding] = [
    Binding("j", "cursor_down", "↓", show=True),
    Binding("k", "cursor_up", "↑", show=True),
    Binding("enter", "select_cursor", "Open", show=True),
]
