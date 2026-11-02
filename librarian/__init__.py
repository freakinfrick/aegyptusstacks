"""The Librarian — agent bridge and context injection for The Stacks."""

from .bridge import LibrarianBridge
from .context import build_context

__all__ = ["LibrarianBridge", "build_context"]
