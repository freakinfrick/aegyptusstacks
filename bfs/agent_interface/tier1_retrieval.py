"""L6 Agent Interface — Tier 1: Retrieval Tools (Pure Reads).

No side effects. No session awareness. Returns coordinate sets with scores.
Every method is idempotent. The Librarian uses these to answer questions
and find texts. Coordinates returned here are the ONLY valid inputs for
Tier 2/3 operations — this is the core anti-hallucination guarantee.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

from bfs.resolver.constants import ENTRANCE_COORD
from bfs.resolver.districts import DISTRICT_LAYOUT
from bfs.topology.graph import TopologyGraph
from bfs.topology.hex_utils import coord_distance
if TYPE_CHECKING:
    from bfs.resolver.resolver import SpatialResolver


@runtime_checkable
class IndexServiceProtocol(Protocol):
    """Minimal interface expected from L5 IndexService (w3a)."""

    def search_lexical(self, query: str, limit: int = 10) -> list[Any]: ...
    def search_metadata(
        self,
        district: str | None = None,
        z_band: int | None = None,
        script_type: str | None = None,
        limit: int = 20,
    ) -> list[Any]: ...
    def get_volume_summary(self, canonical_id: str) -> dict[str, Any]: ...


class RetrievalTools:
    """Pure read operations over the BFS index.

    Every method returns dicts suitable for JSON serialization.
    No state mutation. No session awareness. Idempotent.
    """

    def __init__(
        self,
        index_service: Any,
        resolver: SpatialResolver,
        graph: TopologyGraph,
    ) -> None:
        self.index = index_service
        self.resolver = resolver
        self.graph = graph

    # ------------------------------------------------------------------
    # Search
    # ------------------------------------------------------------------

    def semantic_search(self, query: str, limit: int = 10) -> list[dict[str, Any]]:
        """Find volumes by meaning. Delegates to lexical search for now.

        Future: switch to embedding-based search when IndexService supports it.
        """
        return self.lexical_search(query, field="translation", limit=limit)

    def lexical_search(
        self, query: str, field: str = "translation", limit: int = 10
    ) -> list[dict[str, Any]]:
        """Find volumes by text match.

        Fields: translation, transliteration, name.
        """
        hits = self.index.search_lexical(query, limit=limit)
        results: list[dict[str, Any]] = []
        for hit in hits:
            entry = hit.model_dump() if hasattr(hit, "model_dump") else dict(hit)
            # Enrich with spatial coordinates if available
            cid = entry.get("canonical_id")
            if cid and cid in self.resolver._resolved:
                coord = self.resolver._resolved[cid]
                entry["coord"] = coord
            results.append(entry)
        return results

    def metadata_filter(
        self,
        district: str | None = None,
        z_band: int | None = None,
        script_type: str | None = None,
        limit: int = 20,
    ) -> list[dict[str, Any]]:
        """Filter volumes by metadata fields."""
        # Try IndexService first if available
        if hasattr(self.index, "search_metadata"):
            hits = self.index.search_metadata(
                district=district, z_band=z_band, script_type=script_type, limit=limit
            )
            return [
                h.model_dump() if hasattr(h, "model_dump") else dict(h) for h in hits
            ]

        # Fallback: filter from resolver state directly
        results: list[dict[str, Any]] = []
        state = self.resolver._state
        if state is None:
            return results

        for room_key, volumes in state.room_volumes.items():
            meta = state.room_metadata.get(room_key, {})
            parts = room_key.split(":")
            rq, rr, rz = int(parts[0]), int(parts[1]), int(parts[2])

            if district is not None and meta.get("district") != district:
                continue
            if z_band is not None and rz != z_band:
                continue

            for cid in volumes:
                if len(results) >= limit:
                    break
                results.append(
                    {
                        "canonical_id": cid,
                        "coord": (rq, rr, rz),
                        "district": meta.get("district"),
                        "z_band": rz,
                    }
                )
            if len(results) >= limit:
                break

        return results[:limit]

    # ------------------------------------------------------------------
    # Spatial queries
    # ------------------------------------------------------------------

    def neighborhood(
        self, q: int, r: int, z: int, radius: int = 2
    ) -> list[dict[str, Any]]:
        """All rooms within radius hex-steps of (q, r, z)."""
        center = (q, r, z)
        results: list[dict[str, Any]] = []

        for coord in self.graph._occupied_coords:
            dist = coord_distance(center, coord)
            if dist <= radius:
                room_desc = self.graph.describe_room(coord)
                room_key = self.resolver.get_room_key(*coord)
                meta = {}
                if self.resolver._state:
                    meta = self.resolver._state.room_metadata.get(room_key, {})
                results.append(
                    {
                        "coord": coord,
                        "distance": dist,
                        "door_count": len(room_desc.doors),
                        "volume_count": room_desc.volume_count or meta.get("volume_count", 0),
                        "room_type": room_desc.room_type or meta.get("room_type", "chamber"),
                        "district": room_desc.district or meta.get("district", ""),
                    }
                )

        results.sort(key=lambda r: r["distance"])
        return results

    def pathfind(
        self,
        from_q: int,
        from_r: int,
        from_z: int,
        to_q: int,
        to_r: int,
        to_z: int,
    ) -> dict[str, Any]:
        """Shortest door-connected path between two coordinates."""
        start = (from_q, from_r, from_z)
        end = (to_q, to_r, to_z)
        path = self.graph.pathfind(start, end)

        if not path:
            return {
                "found": False,
                "start": start,
                "end": end,
                "path": [],
                "steps": 0,
            }

        return {
            "found": True,
            "start": start,
            "end": end,
            "path": path,
            "steps": len(path) - 1,
        }

    # ------------------------------------------------------------------
    # Descriptions
    # ------------------------------------------------------------------

    def describe_volume(self, canonical_id: str) -> dict[str, Any]:
        """Summary of a single volume without reading its full content."""
        result: dict[str, Any] = {"canonical_id": canonical_id}

        # Spatial info from resolver
        if canonical_id in self.resolver._resolved:
            coord = self.resolver._resolved[canonical_id]
            result["coord"] = coord
            room_key = self.resolver.get_room_key(*coord)
            if self.resolver._state:
                meta = self.resolver._state.room_metadata.get(room_key, {})
                result["district"] = meta.get("district")
                result["z_band"] = coord[2]
                result["room_type"] = meta.get("room_type")

        # Rich summary from IndexService if available
        if hasattr(self.index, "get_volume_summary"):
            try:
                summary = self.index.get_volume_summary(canonical_id)
                result.update(summary)
            except (KeyError, AttributeError):
                pass

        return result

    def describe_district(self, district: str) -> dict[str, Any]:
        """Overview of a scribal district: boundaries, room count, themes."""
        region = DISTRICT_LAYOUT.get(district)
        if region is None:
            return {"error": f"Unknown district: {district}"}

        result: dict[str, Any] = {
            "name": region.name,
            "description": region.description,
            "center": (region.center_q, region.center_r),
            "radius": region.radius,
        }

        # Stats from resolver state
        if self.resolver._state:
            result["text_count"] = self.resolver._state.district_stats.get(district, 0)
            # Count rooms in this district
            room_count = 0
            for room_key, meta in self.resolver._state.room_metadata.items():
                if meta.get("district") == district:
                    room_count += 1
            result["room_count"] = room_count

        return result

    def list_landmarks(self) -> list[dict[str, Any]]:
        """Named waypoints in the library (major texts, observatory, etc.)."""
        landmarks: list[dict[str, Any]] = []

        # Observatory is always a landmark
        landmarks.append(
            {
                "label": "The Observatory",
                "coord": ENTRANCE_COORD,
                "description": "The entrance to the library. Highest point.",
            }
        )

        # Named landmarks from resolver's loaded data
        if hasattr(self.resolver, "_landmarks"):
            for cid, z_band in self.resolver._landmarks.items():
                entry: dict[str, Any] = {
                    "label": cid,
                    "z_band": z_band,
                }
                if cid in self.resolver._resolved:
                    entry["coord"] = self.resolver._resolved[cid]
                landmarks.append(entry)

        return landmarks
