"""L6 Agent Interface — Tier 2: Spatial Tools (Session State Mutations).

Every operation requires a VALIDATED coordinate — one that exists in the
resolved topology. An agent cannot teleport to a room that doesn't exist.
This is the three-tier separation's core safety property.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

from bfs.topology.graph import TopologyGraph

if TYPE_CHECKING:
    from bfs.adapters.aegyptus import AegyptusAdapter
    from bfs.resolver.resolver import SpatialResolver


Coord = tuple[int, int, int]


@runtime_checkable
class StateManagerProtocol(Protocol):
    """Minimal interface expected from L7 StateManager (w3b)."""

    def get_session(self, session_id: str) -> Any: ...
    def update_position(
        self, session_id: str, new_pos: tuple[int, int, int], via: str = "walk"
    ) -> None: ...
    def add_to_workspace(
        self,
        session_id: str,
        canonical_id: str,
        source_coord: tuple[int, int, int],
    ) -> None: ...


class SpatialTools:
    """Session-mutating spatial operations.

    Every method requires a validated coordinate. Coordinates are validated
    by checking they exist in the resolved topology.
    """

    def __init__(
        self,
        state_manager: Any,
        resolver: SpatialResolver,
        graph: TopologyGraph,
        adapter: AegyptusAdapter,
    ) -> None:
        self.state = state_manager
        self.resolver = resolver
        self.graph = graph
        self.adapter = adapter
        self._valid_coords: set[Coord] | None = None

    # ------------------------------------------------------------------
    # Coordinate validation
    # ------------------------------------------------------------------

    def _validate_coord(self, q: int, r: int, z: int) -> bool:
        """Check coordinate exists in resolved topology."""
        if self._valid_coords is None:
            self._valid_coords = set(self.resolver._occupied)
        return (q, r, z) in self._valid_coords

    def _require_valid_coord(self, q: int, r: int, z: int) -> Coord:
        """Validate and return coord, or raise."""
        if not self._validate_coord(q, r, z):
            raise ValueError(f"Invalid coordinate: ({q},{r},{z})")
        return (q, r, z)

    # ------------------------------------------------------------------
    # Movement
    # ------------------------------------------------------------------

    def teleport(self, session_id: str, q: int, r: int, z: int) -> dict[str, Any]:
        """Instant move to a coordinate. No path traversal.

        Updates session position. Returns the destination room info.
        Requires: coordinate exists in topology.
        """
        coord = self._require_valid_coord(q, r, z)
        self.state.update_position(session_id, coord, via="teleport")
        return {
            "position": coord,
            "room": self._room_info(coord),
        }

    def walk_to(self, session_id: str, q: int, r: int, z: int) -> dict[str, Any]:
        """Navigate step-by-step through doors to a destination.

        Returns the ordered list of rooms traversed (the escort path).
        Updates session position to the final room.
        Updates traversal history for each room visited.
        """
        dest = self._require_valid_coord(q, r, z)
        session = self.state.get_session(session_id)
        current = tuple(session.current_position)

        if current == dest:
            return {"path": [current], "steps": 0, "position": dest}

        path = self.graph.pathfind(current, dest)  # type: ignore[arg-type]
        if not path:
            return {
                "error": "No path found",
                "from": current,
                "to": dest,
                "path": [],
                "steps": 0,
            }

        # Walk each step, updating position along the way
        for step_coord in path[1:]:
            self.state.update_position(session_id, step_coord, via="walk")

        return {
            "path": path,
            "steps": len(path) - 1,
            "position": dest,
            "room": self._room_info(dest),
        }

    def peek_through_door(
        self, session_id: str, direction: str
    ) -> dict[str, Any]:
        """Look through a door without entering.

        Direction: 'N', 'NE', 'SE', 'S', 'SW', 'NW', 'UP', 'DOWN'
        Returns partial room info without full content.
        Does NOT update session position.
        """
        session = self.state.get_session(session_id)
        current: Coord = tuple(session.current_position)  # type: ignore[assignment]

        # Find the door in the requested direction
        doors = self.graph.doors.get(current, [])
        # Normalize direction aliases
        dir_map = {
            "N": "NW", "NE": "NE", "E": "E",
            "SE": "SE", "S": "SW", "SW": "SW",
            "NW": "NW", "W": "W", "UP": "UP", "DOWN": "DOWN",
        }
        target_dir = dir_map.get(direction.upper(), direction.upper())

        matching = [d for d in doors if d.direction == target_dir]
        if not matching:
            return {
                "error": f"No door in direction {direction}",
                "available_doors": [d.direction for d in doors],
            }

        door = matching[0]
        peek_coord: Coord = door.to_coord  # type: ignore[assignment]
        room_key = self.resolver.get_room_key(*peek_coord)
        meta: dict[str, Any] = {}
        if self.resolver._state:
            meta = self.resolver._state.room_metadata.get(room_key, {})

        return {
            "direction": door.direction,
            "coord": peek_coord,
            "room_type": meta.get("room_type", "chamber"),
            "district": meta.get("district", ""),
            "volume_count": meta.get("volume_count", 0),
        }

    # ------------------------------------------------------------------
    # Room interaction
    # ------------------------------------------------------------------

    def describe_room(self, session_id: str) -> dict[str, Any]:
        """Full description of the current room.

        Includes: shelves, volumes, doors, walls, ambient, district context.
        """
        session = self.state.get_session(session_id)
        current: Coord = tuple(session.current_position)  # type: ignore[assignment]
        return self._room_info(current, full=True)

    def fetch_to_workspace(
        self, session_id: str, canonical_id: str
    ) -> dict[str, Any]:
        """Copy a volume to the user's workspace.

        The original stays in place. A provenance thread links the
        copy to its source coordinate.
        """
        # Verify the volume exists in the resolver
        if canonical_id not in self.resolver._resolved:
            raise ValueError(f"Unknown volume: {canonical_id}")

        source_coord = self.resolver._resolved[canonical_id]
        self.state.add_to_workspace(session_id, canonical_id, source_coord)

        return {
            "canonical_id": canonical_id,
            "source_coord": source_coord,
            "status": "gathered",
        }

    def examine_shelf(
        self, session_id: str, shelf_index: int
    ) -> dict[str, Any]:
        """Inspect a specific shelf in the current room.

        Returns volume spines with titles (canonical_ids).
        """
        session = self.state.get_session(session_id)
        current: Coord = tuple(session.current_position)  # type: ignore[assignment]
        room_key = self.resolver.get_room_key(*current)

        if self.resolver._state is None:
            return {"error": "Resolver state not loaded"}

        meta = self.resolver._state.room_metadata.get(room_key, {})
        shelves = meta.get("shelves", [])

        if shelf_index < 0 or shelf_index >= len(shelves):
            return {
                "error": f"Shelf {shelf_index} does not exist",
                "shelf_count": len(shelves),
            }

        volumes = shelves[shelf_index]
        return {
            "shelf_index": shelf_index,
            "volume_count": len(volumes),
            "volumes": volumes,
            "room_coord": current,
        }

    def open_volume(
        self, session_id: str, canonical_id: str
    ) -> dict[str, Any]:
        """Open a volume for reading. Returns full content.

        Updates session state: current_volume = canonical_id.
        """
        if canonical_id not in self.resolver._resolved:
            raise ValueError(f"Unknown volume: {canonical_id}")

        content = self.adapter.get_volume_content(canonical_id)

        # Update session state to track current volume
        session = self.state.get_session(session_id)
        if hasattr(session, "current_volume"):
            session.current_volume = canonical_id
        if hasattr(session, "volumes_opened"):
            session.volumes_opened.add(canonical_id)

        return content.model_dump()

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _room_info(self, coord: Coord, full: bool = False) -> dict[str, Any]:
        """Build room info dict from topology + resolver metadata."""
        room_key = self.resolver.get_room_key(*coord)
        room_desc = self.graph.describe_room(coord)
        meta: dict[str, Any] = {}
        if self.resolver._state:
            meta = self.resolver._state.room_metadata.get(room_key, {})

        info: dict[str, Any] = {
            "coord": coord,
            "room_type": meta.get("room_type", room_desc.room_type),
            "district": meta.get("district", room_desc.district),
            "volume_count": meta.get("volume_count", room_desc.volume_count),
            "doors": [
                {"direction": d.direction, "to": d.to_coord}
                for d in room_desc.doors
            ],
        }

        if full:
            info["walls"] = [
                {"to": w.to_coord, "proximity": w.proximity_score}
                for w in room_desc.walls
            ]
            info["shafts"] = [
                {"direction": s.direction, "to": s.to_coord}
                for s in room_desc.shafts
            ]
            info["shelves"] = meta.get("shelves", [])
            info["palette"] = meta.get("palette")
            info["z_band"] = coord[2]

            # Volume list for this room
            volumes = self.resolver._state.room_volumes.get(room_key, []) if self.resolver._state else []
            info["volumes"] = volumes

        return info
