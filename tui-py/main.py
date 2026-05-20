"""Entry point for the Claude Code Trace Python TUI."""

from __future__ import annotations

import os
import sys

# Ensure the tui-py directory is on sys.path so relative imports work
_dir = os.path.dirname(os.path.abspath(__file__))
if _dir not in sys.path:
    sys.path.insert(0, _dir)

from app import CCTraceApp  # noqa: E402


def main() -> None:
    app = CCTraceApp()
    app.run()


if __name__ == "__main__":
    main()
