"""L5 Index Service — query interface over the resolved library.

Serves four query types:
1. Lexical search (via adapter + coordinate annotation)
2. Metadata filter (district, z_band, script_type)
3. Graph queries (pathfind, neighborhood)
4. Room/volume retrieval

All results are annotated with spatial coordinates from the resolver.
"""

from __future__ import annotations

from typing import Any

from bfs.adapters.aegyptus import AegyptusAdapter
from bfs.resolver.resolver import SpatialResolver
from bfs.topology.graph import TopologyGraph
from bfs.topology.hex_utils import coord_distance
from bfs.udm.models import SearchHit, VolumeContent


class IndexService:
    """Query interface over the resolved library."""

    def __init__(
        self,
        adapter: AegyptusAdapter,
        graph: TopologyGraph,
        resolver: SpatialResolver,
    ) -> None:
        self.adapter = adapter
        self.graph = graph
        self.resolver = resolver

    # ------------------------------------------------------------------
    # Search
    # ------------------------------------------------------------------

    def search_lexical(
        self,
        query: str,
        fields: list[str] | None = None,
        limit: int = 20,
    ) -> list[SearchHit]:
        """Search via adapter, annotate results with coordinates."""
        results = self.adapter.search_lexical(query, fields, limit)
        hits: list[SearchHit] = []
        for r in results:
            coord = None
            district: str | None = None
            z_band: int | None = None
            if r.canonical_id in self.resolver._resolved:
                coord = self.resolver.resolve(r.canonical_id)
                room_key = self.resolver.get_room_key(*coord)
                meta = self.resolver._room_metadata.get(room_key, {})
                district = meta.get("district")
                z_band = coord[2] if coord else None
            hits.append(SearchHit(
                canonical_id=r.canonical_id,
                conventional_name=r.conventional_name,
                score=r.score,
                snippet=r.snippet,
                coord=coord,
                district=district,
                z_band=z_band,
            ))
        return hits

    def filter_metadata(
        self,
        district: str | None = None,
        z_band: int | None = None,
        script_type: str | None = None,
        limit: int = 20,
    ) -> list[SearchHit]:
        """Filter texts by metadata, return with coordinates.

        Scans the resolver state for rooms matching the filter criteria.
        """
        hits: list[SearchHit] = []
        for room_key, meta in self.resolver._room_metadata.items():
            if district and meta.get("district") != district:
                continue
            if z_band is not None and meta.get("z_band") != z_band:
                continue

            volumes = self.resolver._room_volumes.get(room_key, [])
            parts = room_key.split(":")
            coord = (int(parts[0]), int(parts[1]), int(parts[2]))

            for cid in volumes:
                if script_type:
                    cls = self.resolver._classifications.get(cid, {})
                    if cls.get("script_type") != script_type:
                        continue

                cls = self.resolver._classifications.get(cid, {})
                hits.append(SearchHit(
                    canonical_id=cid,
                    conventional_name=cls.get("conventional_name"),
                    score=1.0,
                    coord=coord,
                    district=meta.get("district"),
                    z_band=coord[2],
                ))
                if len(hits) >= limit:
                    return hits
        return hits

    # ------------------------------------------------------------------
    # Graph queries
    # ------------------------------------------------------------------

    def pathfind(
        self,
        start: tuple[int, int, int],
        end: tuple[int, int, int],
    ) -> dict[str, Any]:
        """Pathfind and return path with room descriptions."""
        path = self.graph.pathfind(start, end)
        waypoints: list[dict[str, Any]] = []
        for coord in path:
            room_key = self.resolver.get_room_key(*coord)
            meta = self.resolver._room_metadata.get(room_key, {})
            waypoints.append({
                "coord": coord,
                "room_type": meta.get("room_type", "chamber"),
                "district": meta.get("district", "unknown"),
                "volume_count": meta.get("volume_count", 0),
            })
        return {
            "path": [list(c) for c in path],
            "distance": len(path),
            "waypoints": waypoints,
        }

    def neighborhood(
        self,
        coord: tuple[int, int, int],
        radius: int = 2,
    ) -> list[dict[str, Any]]:
        """All rooms within radius steps."""
        rooms: list[dict[str, Any]] = []
        for room_key, meta in self.resolver._room_metadata.items():
            parts = room_key.split(":")
            rc = (int(parts[0]), int(parts[1]), int(parts[2]))
            dist = coord_distance(coord, rc)
            if dist <= radius:
                rooms.append({
                    "coord": rc,
                    "distance": dist,
                    "room_type": meta.get("room_type", "chamber"),
                    "district": meta.get("district", "unknown"),
                    "volume_count": meta.get("volume_count", 0),
                    "doors": len(self.graph.doors.get(rc, [])),
                })
        rooms.sort(key=lambda x: x["distance"])
        return rooms

    # ------------------------------------------------------------------
    # Retrieval
    # ------------------------------------------------------------------

    def get_room_metadata(
        self, q: int, r: int, z: int
    ) -> dict[str, Any] | None:
        """Get room metadata for a coordinate."""
        room_key = self.resolver.get_room_key(q, r, z)
        meta = self.resolver._room_metadata.get(room_key)
        if meta is None:
            return None
        volumes = self.resolver._room_volumes.get(room_key, [])
        door_edges = self.graph.doors.get((q, r, z), [])
        wall_edges = self.graph.walls.get((q, r, z), [])
        shaft_edges = self.graph.shafts.get((q, r, z), [])

        # Enrich volumes with physical_type for the Godot stamper
        volumes_typed: list[dict[str, str]] = []
        for cid in volumes:
            cls = self.resolver._classifications.get(cid, {})
            volumes_typed.append({
                "id": cid,
                "physical_type": cls.get("physical_type", "unknown"),
            })

        # Resolve cluster identity for room naming
        cluster_id = ""
        cluster_signature: list[str] = []
        for cid, room_keys in self.resolver._cluster_rooms.items():
            if room_key in room_keys:
                cluster_id = cid
                break
        if cluster_id:
            # Get signature lemmata from any text in this cluster
            for text_id, text_data in self.resolver._clusters.items():
                if text_data.get("cluster_id") == cluster_id:
                    cluster_signature = text_data.get("cluster_signature_lemmata", [])
                    if cluster_signature:
                        break

        return {
            "room_key": room_key,
            "q": q,
            "r": r,
            "z": z,
            **meta,
            "cluster_id": cluster_id,
            "cluster_signature": cluster_signature[:5],
            "volumes": volumes,
            "volumes_typed": volumes_typed,
            "door_edges": [
                {"from_coord": list(d.from_coord), "to_coord": list(d.to_coord), "direction": d.direction}
                for d in door_edges
            ],
            "wall_edges": [
                {"from_coord": list(w.from_coord), "to_coord": list(w.to_coord), "proximity_score": w.proximity_score}
                for w in wall_edges
            ],
            "shaft_edges": [
                {"from_coord": list(s.from_coord), "to_coord": list(s.to_coord), "direction": s.direction}
                for s in shaft_edges
            ],
        }

    def get_volume(self, canonical_id: str) -> VolumeContent:
        """Fetch full volume content from the adapter."""
        return self.adapter.get_volume_content(canonical_id)

    def get_room_texts(self, q: int, r: int, z: int) -> dict[str, Any]:
        """Batch-fetch all text content for a room — compact, LLM-friendly.

        Returns room identity + all texts with transliteration/translation.
        No tokens, no morphology, no metadata bloat.  Two SQL queries total.
        """
        room_key = self.resolver.get_room_key(q, r, z)
        meta = self.resolver._room_metadata.get(room_key)
        if meta is None:
            raise KeyError(f"No room at ({q}, {r}, {z})")

        volume_ids = self.resolver._room_volumes.get(room_key, [])

        # Physical types from classifications
        type_map: dict[str, str] = {}
        for cid in volume_ids:
            cls = self.resolver._classifications.get(cid, {})
            ptype = cls.get("physical_type", "")
            if ptype and ptype != "unknown":
                type_map[cid] = ptype

        texts = self.adapter.get_batch_texts_compact(volume_ids)

        # Enrich with physical_type
        for t in texts:
            ptype = type_map.get(t["id"], "")
            if ptype:
                t["type"] = ptype

        return {
            "room": room_key,
            "district": meta.get("district", "unknown"),
            "room_type": meta.get("room_type", "unknown"),
            "volume_count": len(volume_ids),
            "texts": texts,
        }

    def get_district_info(self, district: str) -> dict[str, Any]:
        """Return statistics for a district."""
        room_count = 0
        text_count = 0
        z_bands: dict[int, int] = {}

        for room_key, meta in self.resolver._room_metadata.items():
            if meta.get("district") != district:
                continue
            room_count += 1
            vol_count = meta.get("volume_count", 0)
            text_count += vol_count
            parts = room_key.split(":")
            z = int(parts[2])
            z_bands[z] = z_bands.get(z, 0) + vol_count

        from bfs.resolver.districts import DISTRICT_LAYOUT
        region = DISTRICT_LAYOUT.get(district)
        return {
            "district": district,
            "description": region.description if region else "",
            "center_q": region.center_q if region else 0,
            "center_r": region.center_r if region else 0,
            "radius": region.radius if region else 0,
            "room_count": room_count,
            "text_count": text_count,
            "z_band_distribution": {str(k): v for k, v in sorted(z_bands.items())},
        }
