"""Session-source plugins. Each one lives in ``plugins/<id>/``.

To add a source, create its folder and add its class to ``REGISTRY``. The
order here is the order of the sections in the picker.
"""

from plugins.antigravity import AntigravityPlugin
from plugins.base import SourcePlugin
from plugins.claude import ClaudePlugin
from plugins.codex import CodexPlugin

REGISTRY: tuple[type[SourcePlugin], ...] = (ClaudePlugin, AntigravityPlugin, CodexPlugin)
