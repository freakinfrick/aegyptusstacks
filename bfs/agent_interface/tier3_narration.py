"""L6 Agent Interface — Tier 3: Narration Tools (Overlay Mutations).

Cosmetic and additive. Never affect base topology. All changes are
per-user overlay state — annotations, bookmarks, highlights. These
persist across sessions but are isolated from the shared library.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any


Coord = tuple[int, int, int]


class NarrationTools:
    """Overlay mutations. Additive and cosmetic.

    Never modify base topology. All changes are per-session state
    managed by the StateManager.
    """

    def __init__(self, state_manager: Any) -> None:
        self.state = state_manager

    def annotate(
        self, session_id: str, canonical_id: str, note: str
    ) -> dict[str, Any]:
        """Add a marginal note to a volume.

        Renders in the reading view as a side annotation.
        """
        session = self.state.get_session(session_id)
        annotation = {
            "canonical_id": canonical_id,
            "note": note,
            "created_at": datetime.now(timezone.utc).isoformat(),
        }

        # Store in session's workspace annotations
        if hasattr(session, "workspace_annotations"):
            session.workspace_annotations.append(annotation)

        return {
            "status": "annotated",
            "canonical_id": canonical_id,
            "note": note,
        }

    def bookmark(
        self, session_id: str, q: int, r: int, z: int, label: str
    ) -> dict[str, Any]:
        """Name a room for fast return.

        'Take me to [label]' becomes a teleport target.
        """
        coord: Coord = (q, r, z)
        session = self.state.get_session(session_id)

        if hasattr(session, "bookmarks"):
            session.bookmarks[label] = coord

        return {
            "status": "bookmarked",
            "label": label,
            "coord": coord,
        }

    def highlight_shelf(
        self,
        session_id: str,
        q: int,
        r: int,
        z: int,
        shelf_index: int,
        color: str = "gold",
    ) -> dict[str, Any]:
        """Visually mark a shelf for later reference.

        Highlights are overlay state — they don't affect the base topology.
        """
        coord: Coord = (q, r, z)

        # Store highlight in session (StateManager handles persistence)
        session = self.state.get_session(session_id)
        highlight = {
            "coord": coord,
            "shelf_index": shelf_index,
            "color": color,
            "created_at": datetime.now(timezone.utc).isoformat(),
        }

        # Use a highlights list on the session if available, otherwise
        # fall back to a generic overlay store
        if hasattr(session, "shelf_highlights"):
            session.shelf_highlights.append(highlight)

        return {
            "status": "highlighted",
            "coord": coord,
            "shelf_index": shelf_index,
            "color": color,
        }
