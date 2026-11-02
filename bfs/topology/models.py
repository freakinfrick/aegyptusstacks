"""Pydantic models for topology edges and room descriptions."""

from __future__ import annotations

from pydantic import BaseModel

Coord = tuple[int, int, int]  # (q, r, z)


class DoorEdge(BaseModel):
    """Walkable edge between two rooms."""

    from_coord: tuple[int, int, int]
    to_coord: tuple[int, int, int]
    direction: str  # E, NE, NW, W, SW, SE, UP, DOWN


class WallEdge(BaseModel):
    """Implicit (non-walkable) edge between adjacent rooms."""

    from_coord: tuple[int, int, int]
    to_coord: tuple[int, int, int]
    proximity_score: float = 0.0  # semantic proximity


class ShaftEdge(BaseModel):
    """Vertical edge between z-levels."""

    from_coord: tuple[int, int, int]
    to_coord: tuple[int, int, int]
    direction: str  # UP or DOWN


class RoomDescription(BaseModel):
    """Full description of a room and its connections."""

    coord: tuple[int, int, int]
    doors: list[DoorEdge] = []
    walls: list[WallEdge] = []
    shafts: list[ShaftEdge] = []
    volume_count: int = 0
    room_type: str = "chamber"
    district: str = ""
