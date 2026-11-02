"""L7 Runtime State Manager — Pydantic session models."""

from __future__ import annotations

from datetime import datetime, timezone
from uuid import uuid4

from pydantic import BaseModel, Field


class TraversalEntry(BaseModel):
    """A single step in the traversal history."""

    coord: tuple[int, int, int]
    entered_at: datetime
    exited_at: datetime | None = None
    dwell_time_seconds: float | None = None
    entered_via: str  # "walk", "teleport", "escort", "shaft"


class WorkspaceCopy(BaseModel):
    """A volume copy in the user's workspace."""

    canonical_id: str
    source_coord: tuple[int, int, int]
    fetched_at: datetime
    annotations: list[str] = []
    grouping: str | None = None  # user-assigned group label


class Annotation(BaseModel):
    """A marginal note attached to a volume."""

    canonical_id: str
    note: str
    created_at: datetime


class Bookmark(BaseModel):
    """A named room for fast return."""

    label: str
    coord: tuple[int, int, int]
    created_at: datetime


class SessionState(BaseModel):
    """Per-session ephemeral state."""

    session_id: str = Field(default_factory=lambda: str(uuid4()))
    user_id: str = "default"  # single-user for hackathon

    # Position
    current_position: tuple[int, int, int]  # (q, r, z)
    current_volume: str | None = None  # canonical_id if reading
    current_shelf: int | None = None  # shelf index if examining

    # History
    traversal_history: list[TraversalEntry] = []
    rooms_visited: list[tuple[int, int, int]] = []  # list, not set — JSON serializable
    volumes_opened: set[str] = Field(default_factory=set)

    # Workspace
    workspace_volumes: list[WorkspaceCopy] = []
    workspace_annotations: list[Annotation] = []

    # Overlays
    bookmarks: dict[str, tuple[int, int, int]] = {}

    # Timestamps
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    last_active: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    interaction_count: int = 0
