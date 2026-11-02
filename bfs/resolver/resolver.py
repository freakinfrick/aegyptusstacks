"""L3 Spatial Resolver — maps every text to (q, r, z) coordinates.

The single most important component in the BFS system. Given Phase 1
classification and cluster data, deterministically assigns every text
in the Aegyptus corpus to a hex-grid coordinate (q, r, z).

Determinism guarantee: same seed + same input → same output, always.
"""

from __future__ import annotations

import json
import math
import random
from collections import defaultdict
from pathlib import Path

from pydantic import BaseModel

from bfs.resolver.constants import (
    CANONICAL_SEED,
    CLUSTER_BRIDGE_DISTANCE,
    HEX_DIRECTIONS,
    MAX_VOLUMES_PER_ROOM,
    SHELVES_PER_ROOM,
    VOLUMES_PER_SHELF,
)
from bfs.resolver.districts import DISTRICT_LAYOUT, DistrictRegion
from bfs.resolver.z_bands import compute_z_band


class ResolverState(BaseModel):
    """Persisted state of the resolved library."""

    seed: int
    room_assignments: dict[str, tuple[int, int, int]]  # canonical_id -> (q,r,z)
    room_volumes: dict[str, list[str]]  # room_key "q:r:z" -> [canonical_ids]
    room_metadata: dict[str, dict]  # room_key -> metadata dict
    cluster_rooms: dict[str, list[str]]  # cluster_id -> [room_keys]
    district_stats: dict[str, int]  # district -> text count
    z_band_stats: dict[str, int]  # z_band -> text count
    total_rooms: int
    total_texts: int
    # 2D topology: rotunda room_key -> [cluster_a_id, cluster_b_id]
    rotunda_bridges: dict[str, list[str]] = {}


# Ambient palette by z-level (from Phase 2 spec §2.8)
Z_PALETTES: dict[str, str] = {
    "surface": "warm_amber",       # z >= 0
    "professional": "warm_amber",  # z = -1 to -2
    "ritual": "sage_green",        # z = -3 to -4
    "cosmic": "ultramarine",       # z = -5 to -6
    "primordial": "phosphor_teal", # z = -7
}


def _palette_for_z(z: int) -> str:
    if z >= 0:
        return Z_PALETTES["surface"]
    if z >= -2:
        return Z_PALETTES["professional"]
    if z >= -4:
        return Z_PALETTES["ritual"]
    if z >= -6:
        return Z_PALETTES["cosmic"]
    return Z_PALETTES["primordial"]


def _room_type(volume_count: int, z: int, door_count: int) -> str:
    """Determine room type from contents and connectivity."""
    if z > 0:
        if z == 3:
            return "rotunda"
        return "atrium"
    if volume_count <= 15 and door_count <= 1:
        return "terminus"
    if volume_count <= 30 and door_count == 2:
        return "hallway"
    if door_count >= 3:
        return "junction"
    if z <= -5:
        return "vault"
    if volume_count >= 40:
        return "gallery"
    return "chamber"


class SpatialResolver:
    """Maps every text in the corpus to (q, r, z) coordinates."""

    def __init__(self, seed: int = CANONICAL_SEED, data_dir: str | Path = "data/phase1"):
        self.seed = seed
        self.rng = random.Random(seed)
        self.data_dir = Path(data_dir)
        self._load_phase1_data()
        self._resolved: dict[str, tuple[int, int, int]] = {}
        self._room_volumes: dict[str, list[str]] = defaultdict(list)
        self._room_metadata: dict[str, dict] = {}
        self._cluster_rooms: dict[str, list[str]] = defaultdict(list)
        self._occupied: set[tuple[int, int, int]] = set()
        self._rotunda_bridges: dict[str, list[str]] = {}  # room_key -> [cluster_a, cluster_b]
        self._state: ResolverState | None = None

    # ------------------------------------------------------------------
    # Data loading
    # ------------------------------------------------------------------

    def _load_phase1_data(self) -> None:
        """Load classifications, clusters, landmarks, districts from Phase 1."""
        with open(self.data_dir / "text_classifications.json") as f:
            self._classifications: dict[str, dict] = json.load(f)

        with open(self.data_dir / "clusters.json") as f:
            self._clusters: dict[str, dict] = json.load(f)

        with open(self.data_dir / "landmarks.json") as f:
            landmarks_list: list[dict] = json.load(f)
        self._landmarks: dict[str, int] = {
            lm["canonical_id"]: lm["z_band"] for lm in landmarks_list
        }

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def resolve_all(self, canonical_ids: list[str] | None = None) -> ResolverState:
        """Bulk resolve the entire corpus. THE main function.

        Args:
            canonical_ids: List of text IDs to resolve. If None, uses all
                texts from the classifications file.

        Returns:
            ResolverState with all assignments.
        """
        if canonical_ids is None:
            canonical_ids = list(self._classifications.keys())

        # Reset state for fresh resolution
        self._resolved.clear()
        self._room_volumes.clear()
        self._room_metadata.clear()
        self._cluster_rooms.clear()
        self._occupied.clear()
        self._rotunda_bridges.clear()
        self.rng = random.Random(self.seed)

        # Step 1: Compute z-band for every text (CHRONOLOGICAL era mapping)
        period_labels = getattr(self, "_period_labels", {})
        z_for: dict[str, int] = {}
        for cid in canonical_ids:
            z_for[cid] = compute_z_band(
                canonical_id=cid,
                period_label=period_labels.get(cid),
                landmarks=self._landmarks,
            )

        # Step 2: Group texts by district
        district_texts: dict[str, list[str]] = defaultdict(list)
        for cid in canonical_ids:
            cls = self._classifications.get(cid, {})
            district = cls.get("district", "unplaced")
            district_texts[district].append(cid)

        # Step 3: For each district, group by cluster, then place rooms
        # Sort district names for deterministic iteration
        for district_name in sorted(district_texts.keys()):
            cids_in_district = district_texts[district_name]
            self._resolve_district(district_name, cids_in_district, z_for)

        # Step 4: Build index floors (z > 0)
        self._build_index_floors()

        # Step 5: Compute room metadata (door counts, types, palettes)
        self._compute_room_metadata()

        # Step 6: Build stats and state
        district_stats: dict[str, int] = {}
        for d, cids in district_texts.items():
            district_stats[d] = len(cids)

        z_band_stats: dict[str, int] = defaultdict(int)
        for z in z_for.values():
            z_band_stats[str(z)] += 1

        self._state = ResolverState(
            seed=self.seed,
            room_assignments=dict(self._resolved),
            room_volumes={k: list(v) for k, v in self._room_volumes.items()},
            room_metadata=dict(self._room_metadata),
            cluster_rooms={k: list(v) for k, v in self._cluster_rooms.items()},
            district_stats=district_stats,
            z_band_stats=dict(z_band_stats),
            total_rooms=len(self._room_volumes),
            total_texts=len(self._resolved),
            rotunda_bridges=dict(self._rotunda_bridges),
        )
        return self._state

    def resolve(self, canonical_id: str) -> tuple[int, int, int]:
        """Get coordinates for a single text. Must call resolve_all first."""
        if canonical_id not in self._resolved:
            raise KeyError(
                f"Text {canonical_id!r} not resolved. Call resolve_all() first."
            )
        return self._resolved[canonical_id]

    @staticmethod
    def get_room_key(q: int, r: int, z: int) -> str:
        return f"{q}:{r}:{z}"

    def save_state(self, path: Path) -> None:
        """Persist ResolverState to JSON."""
        if self._state is None:
            raise RuntimeError("No state to save. Call resolve_all() first.")
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w") as f:
            f.write(self._state.model_dump_json(indent=2))

    def load_state(self, path: Path) -> None:
        """Load previously resolved state."""
        with open(path) as f:
            self._state = ResolverState.model_validate_json(f.read())
        self._resolved = dict(self._state.room_assignments)
        self._room_volumes = defaultdict(list)
        for k, v in self._state.room_volumes.items():
            self._room_volumes[k] = list(v)
        self._room_metadata = dict(self._state.room_metadata)
        self._cluster_rooms = defaultdict(list)
        for k, v in self._state.cluster_rooms.items():
            self._cluster_rooms[k] = list(v)
        self._occupied = set()
        for coord in self._resolved.values():
            self._occupied.add(coord)

    # ------------------------------------------------------------------
    # Internal: district resolution
    # ------------------------------------------------------------------

    def _resolve_district(
        self,
        district_name: str,
        cids: list[str],
        z_for: dict[str, int],
    ) -> None:
        """Place all texts in a district onto the hex grid."""
        region = DISTRICT_LAYOUT.get(district_name)
        if region is None:
            region = DISTRICT_LAYOUT["unplaced"]

        # Group by cluster within this district
        cluster_texts: dict[str, list[str]] = defaultdict(list)
        for cid in cids:
            cl = self._clusters.get(cid, {})
            cluster_id = cl.get("cluster_id", f"{district_name}_unclustered")
            cluster_texts[cluster_id].append(cid)

        # Sort cluster IDs for deterministic ordering, largest first for
        # better placement near center
        sorted_clusters = sorted(
            cluster_texts.keys(),
            key=lambda c: (-len(cluster_texts[c]), c),
        )

        # Compute cluster positions within the district
        cluster_positions = self._compute_cluster_positions(
            region, sorted_clusters, cluster_texts
        )

        # Place rooms for each cluster
        for cluster_id in sorted_clusters:
            texts = cluster_texts[cluster_id]
            base_q, base_r = cluster_positions[cluster_id]
            self._place_cluster_rooms(
                cluster_id, texts, base_q, base_r, z_for, district_name
            )

        # Place connector rotundas between adjacent clusters
        if len(sorted_clusters) > 1:
            self._place_rotundas(district_name, sorted_clusters, cluster_positions)

    def _compute_cluster_positions(
        self,
        region: DistrictRegion,
        sorted_clusters: list[str],
        cluster_texts: dict[str, list[str]],
    ) -> dict[str, tuple[int, int]]:
        """Assign each cluster a (q, r) position within the district.

        Uses a spiral layout from district center, spacing clusters
        proportional to their room count so they don't overlap.
        """
        positions: dict[str, tuple[int, int]] = {}

        if len(sorted_clusters) == 1:
            positions[sorted_clusters[0]] = (region.center_q, region.center_r)
            return positions

        # Spiral placement: rings of hex cells radiating from center
        # Each cluster gets a position on the spiral, with deterministic
        # jitter from the seeded RNG
        ring_cells = _hex_spiral_coords(
            region.center_q, region.center_r, max_ring=region.radius
        )

        # Estimate how many hex cells each cluster needs (for spacing)
        cluster_radii: dict[str, int] = {}
        for cid in sorted_clusters:
            room_count = math.ceil(len(cluster_texts[cid]) / MAX_VOLUMES_PER_ROOM)
            cluster_radii[cid] = max(1, math.ceil(math.sqrt(room_count)))

        # Place clusters along the spiral, skipping cells to maintain spacing
        cell_idx = 0
        for cluster_id in sorted_clusters:
            if cell_idx >= len(ring_cells):
                # Overflow: extend beyond district radius
                rng_local = random.Random(self.seed ^ hash(cluster_id))
                dq = rng_local.randint(-region.radius, region.radius)
                dr = rng_local.randint(-region.radius, region.radius)
                positions[cluster_id] = (region.center_q + dq, region.center_r + dr)
                continue

            q, r = ring_cells[cell_idx]

            # Deterministic jitter
            rng_local = random.Random(self.seed ^ hash(cluster_id))
            dq = rng_local.randint(-1, 1)
            dr = rng_local.randint(-1, 1)
            positions[cluster_id] = (q + dq, r + dr)

            # Skip cells proportional to cluster radius
            cell_idx += max(2, cluster_radii[cluster_id] * 2)

        return positions

    def _place_cluster_rooms(
        self,
        cluster_id: str,
        texts: list[str],
        base_q: int,
        base_r: int,
        z_for: dict[str, int],
        district_name: str,
    ) -> None:
        """Place texts into rooms laid out as a 2D hex patch from base position."""
        # Sort texts deterministically (by canonical_id) for stable assignment
        texts_sorted = sorted(texts)

        # Group by z-band within this cluster
        z_groups: dict[int, list[str]] = defaultdict(list)
        for cid in texts_sorted:
            z_groups[z_for[cid]].append(cid)

        # Place chronological z-levels (z<0) FIRST so they get ideal positions
        # near the cluster's base. z=0 (undated, 70% of corpus) goes last —
        # it's the most congested plane and shaft traversal doesn't cross it.
        # Within chronological levels, ascending (deepest first = least contested).
        z_order = sorted(
            z_groups.keys(),
            key=lambda z: (0 if z < 0 else 1, z),
        )

        for z in z_order:
            z_texts = z_groups[z]
            room_count = math.ceil(len(z_texts) / MAX_VOLUMES_PER_ROOM)

            # Lay out rooms as a 2D hex-disk patch
            patch = self._lay_patch(base_q, base_r, z, room_count)

            # Distribute texts across rooms in this patch
            for room_idx, (rq, rr) in enumerate(patch):
                start = room_idx * MAX_VOLUMES_PER_ROOM
                end = min(start + MAX_VOLUMES_PER_ROOM, len(z_texts))
                room_texts = z_texts[start:end]

                coord = (rq, rr, z)
                room_key = self.get_room_key(rq, rr, z)

                for cid in room_texts:
                    self._resolved[cid] = coord
                    self._room_volumes[room_key].append(cid)

                self._cluster_rooms[cluster_id].append(room_key)
                self._occupied.add(coord)

    def _lay_patch(
        self, base_q: int, base_r: int, z: int, count: int
    ) -> list[tuple[int, int]]:
        """Lay out `count` rooms as a filled hex disk centered near (base_q, base_r).

        Rooms are placed in spiral order from center outward, forming a compact
        2D arrangement. Skips occupied cells. Within each ring, cells are sorted
        by distance to (base_q, base_r) so that congestion-displaced centers
        still gravitate back toward the cluster's canonical anchor. This keeps
        centroids aligned across z-levels for semantic stacking.
        """
        from bfs.resolver.constants import hex_distance as _hex_dist

        if count <= 0:
            return []

        # Find unoccupied center
        cq, cr = self._find_unoccupied_near(base_q, base_r, z)

        patch: list[tuple[int, int]] = []
        patch.append((cq, cr))
        self._occupied.add((cq, cr, z))

        if count == 1:
            return patch

        # Compute radius needed: area of hex disk at radius R = 3R^2 + 3R + 1
        # Solve for R given count, with headroom for occupied cells
        radius = max(1, math.ceil(math.sqrt(count / math.pi)))

        # Spiral outward ring by ring.
        # Within each ring, prefer cells closer to (base_q, base_r) — the
        # cluster's canonical position. This prevents congestion at dense
        # z-levels from displacing the patch centroid away from the anchor.
        anchor = (base_q, base_r)
        for ring in range(1, radius + 20):  # +20 headroom for collisions
            if len(patch) >= count:
                break
            ring_cells = _hex_ring(cq, cr, ring)
            ring_cells.sort(key=lambda c: _hex_dist(c, anchor))
            for cell_q, cell_r in ring_cells:
                if (cell_q, cell_r, z) not in self._occupied:
                    patch.append((cell_q, cell_r))
                    self._occupied.add((cell_q, cell_r, z))
                    if len(patch) >= count:
                        break

        return patch

    def _place_rotundas(
        self,
        district_name: str,
        sorted_clusters: list[str],
        cluster_positions: dict[str, tuple[int, int]],
    ) -> None:
        """Place rotunda rooms between adjacent clusters to form a district spine.

        Strategy:
        1. Build proximity graph of clusters (nearest-room distance <= threshold)
        2. Compute MST → district spine ensuring all clusters are reachable
        3. Place a rotunda at the midpoint of each spine edge
        """
        from bfs.resolver.constants import hex_distance as _hex_dist

        # Collect per-cluster room coords (data-floor only, grouped by z)
        cluster_coords: dict[str, list[tuple[int, int, int]]] = {}
        for cid in sorted_clusters:
            rooms = self._cluster_rooms.get(cid, [])
            coords = []
            for rk in rooms:
                parts = rk.split(":")
                coords.append((int(parts[0]), int(parts[1]), int(parts[2])))
            cluster_coords[cid] = coords

        # Find the most common z-level across all clusters in this district
        all_z: list[int] = []
        for coords in cluster_coords.values():
            all_z.extend(c[2] for c in coords)
        if not all_z:
            return
        z_counts: dict[int, int] = defaultdict(int)
        for z in all_z:
            z_counts[z] += 1
        primary_z = max(z_counts, key=lambda z: z_counts[z])

        # Build edges between all cluster pairs using position distances.
        # Close clusters use actual room-to-room distance; far clusters use
        # center-to-center. This guarantees the MST spans all clusters.
        all_edges: list[tuple[str, str, int]] = []
        cluster_list = list(sorted_clusters)
        for i in range(len(cluster_list)):
            ca = cluster_list[i]
            coords_a = cluster_coords[ca]
            pos_a = cluster_positions[ca]
            for j in range(i + 1, len(cluster_list)):
                cb = cluster_list[j]
                coords_b = cluster_coords[cb]
                pos_b = cluster_positions[cb]

                # Use actual room distance for nearby clusters (more accurate)
                if coords_a and coords_b:
                    pos_dist = _hex_dist(pos_a, pos_b)
                    if pos_dist <= CLUSTER_BRIDGE_DISTANCE * 5:
                        dist = _min_cluster_distance(coords_a, coords_b)
                    else:
                        dist = pos_dist
                else:
                    dist = _hex_dist(pos_a, pos_b)
                all_edges.append((ca, cb, dist))

        # Kruskal's MST — guarantees all clusters connected within district
        spine_edges = _kruskal_mst(cluster_list, all_edges)

        # Place a rotunda for each MST edge
        for ca, cb, _dist in spine_edges:
            coords_a = cluster_coords[ca]
            coords_b = cluster_coords[cb]
            if not coords_a or not coords_b:
                continue

            # Find closest room pair between the two clusters
            best_a, best_b = _closest_pair(coords_a, coords_b)

            # Rotunda z-level: use the z of the closest pair (or primary_z)
            rz = best_a[2] if best_a[2] == best_b[2] else primary_z

            # Place rotunda ADJACENT to the nearest cluster room (not at midpoint).
            # Prefer a cell adjacent to best_a that is also closer to best_b.
            rq, rr = self._find_bridge_position(best_a, best_b, rz)
            coord = (rq, rr, rz)
            room_key = self.get_room_key(*coord)

            self._occupied.add(coord)

            # Rotundas carry representative texts from both bridged clusters
            rotunda_volumes: list[str] = []
            for cid in (ca, cb):
                c_rooms = self._cluster_rooms.get(cid, [])
                rotunda_volumes.extend(self._collect_cluster_volumes(c_rooms, 10))
            rotunda_volumes = rotunda_volumes[:20]

            self._room_volumes[room_key] = rotunda_volumes
            self._room_metadata[room_key] = {
                "room_type": "rotunda",
                "district": district_name,
                "volume_count": len(rotunda_volumes),
                "bridges": [ca, cb],
                "palette": _palette_for_z(rz),
                "z_band": rz,
                "door_count": 0,
                "shelves": self._compute_shelves(rotunda_volumes),
            }
            self._cluster_rooms[f"__rotunda_{district_name}"].append(room_key)
            self._rotunda_bridges[room_key] = [ca, cb]

    def _find_bridge_position(
        self,
        best_a: tuple[int, int, int],
        best_b: tuple[int, int, int],
        rz: int,
    ) -> tuple[int, int]:
        """Find a rotunda position adjacent to best_a and as close to best_b as possible.

        This ensures the rotunda can form a physical doorway to at least one cluster room.
        Falls back to _find_unoccupied_near(midpoint) if no adjacent cell is free.
        """
        from bfs.resolver.constants import hex_distance as _hex_dist

        aq, ar = best_a[0], best_a[1]
        bq, br = best_b[0], best_b[1]

        # Try all 6 cells adjacent to best_a, prefer closest to best_b
        candidates = []
        for nq, nr in _hex_ring(aq, ar, 1):
            if (nq, nr, rz) not in self._occupied:
                d = _hex_dist((nq, nr), (bq, br))
                candidates.append((d, nq, nr))

        if candidates:
            candidates.sort()
            return (candidates[0][1], candidates[0][2])

        # Fallback: find unoccupied near midpoint (original behavior)
        mid_q = (aq + bq) // 2
        mid_r = (ar + br) // 2
        return self._find_unoccupied_near(mid_q, mid_r, rz)

    def _find_unoccupied_near(self, q: int, r: int, z: int) -> tuple[int, int]:
        """Find the nearest unoccupied hex cell to (q, r) at z-level z.

        Within each ring, candidates are sorted by distance to (q, r) so
        displacement direction stays as close to the target as possible.
        This prevents congestion from pushing centers in arbitrary directions.
        """
        from bfs.resolver.constants import hex_distance as _hex_dist

        if (q, r, z) not in self._occupied:
            return (q, r)
        target = (q, r)
        # BFS outward in rings, prefer cells closest to target
        for ring in range(1, 200):
            candidates = [
                (nq, nr) for nq, nr in _hex_ring(q, r, ring)
                if (nq, nr, z) not in self._occupied
            ]
            if candidates:
                candidates.sort(key=lambda c: _hex_dist(c, target))
                return candidates[0]
        # Should never reach here for a reasonable corpus
        raise RuntimeError(f"No free cell near ({q},{r},{z}) within 200 rings")

    # ------------------------------------------------------------------
    # Internal: index floors
    # ------------------------------------------------------------------

    def _build_index_floors(self) -> None:
        """Create index rooms at z=+1 (per cluster), z=+2 (per district), z=+3 (observatory).

        Every room gets representative texts from its summarized scope:
        - z=+1 atria: first N texts from their cluster (curated sample)
        - z=+2 atria: texts drawn from each cluster in the district
        - z=+3 observatory: landmark texts (corpus-wide canonical works)
        """
        MAX_INDEX_VOLUMES = 20  # representative sample per index room

        # z=+1: one room per cluster — carries cluster's representative texts
        for cluster_id in sorted(self._cluster_rooms.keys()):
            room_keys = self._cluster_rooms[cluster_id]
            if not room_keys:
                continue
            # Position: centroid of the cluster's data-floor rooms
            qs, rs = [], []
            for rk in room_keys:
                parts = rk.split(":")
                qs.append(int(parts[0]))
                rs.append(int(parts[1]))
            cq = round(sum(qs) / len(qs))
            cr = round(sum(rs) / len(rs))
            cq, cr = self._find_unoccupied_near(cq, cr, 1)
            coord = (cq, cr, 1)
            room_key = self.get_room_key(*coord)
            self._occupied.add(coord)

            # Pull representative texts from cluster's data-floor rooms
            cluster_volumes = self._collect_cluster_volumes(room_keys, MAX_INDEX_VOLUMES)

            self._room_volumes[room_key] = cluster_volumes
            self._room_metadata[room_key] = {
                "room_type": "atrium",
                "district": cluster_id.rsplit("_", 1)[0] if "_" in cluster_id else "unknown",
                "index_level": 1,
                "summarizes_cluster": cluster_id,
                "volume_count": len(cluster_volumes),
                "shelves": self._compute_shelves(cluster_volumes),
            }
            self._cluster_rooms[f"__index_z1_{cluster_id}"] = [room_key]

        # z=+2: one room per district — draws from each cluster
        for district_name in sorted(DISTRICT_LAYOUT.keys()):
            if district_name == "unplaced":
                continue
            region = DISTRICT_LAYOUT[district_name]
            cq, cr = self._find_unoccupied_near(region.center_q, region.center_r, 2)
            coord = (cq, cr, 2)
            room_key = self.get_room_key(*coord)
            self._occupied.add(coord)

            # Gather representative texts from all clusters in this district
            district_volumes: list[str] = []
            for cid, rkeys in self._cluster_rooms.items():
                if cid.startswith("__"):
                    continue
                if cid.startswith(district_name):
                    for rk in rkeys[:1]:  # first room of each cluster
                        district_volumes.extend(self._room_volumes.get(rk, [])[:3])
                if len(district_volumes) >= MAX_INDEX_VOLUMES:
                    break
            district_volumes = district_volumes[:MAX_INDEX_VOLUMES]

            self._room_volumes[room_key] = district_volumes
            self._room_metadata[room_key] = {
                "room_type": "atrium",
                "district": district_name,
                "index_level": 2,
                "summarizes_district": district_name,
                "volume_count": len(district_volumes),
                "shelves": self._compute_shelves(district_volumes),
            }

        # z=+3: Observatory — empty origin room, the apex from which all descends
        obs_q, obs_r = self._find_unoccupied_near(0, 0, 3)
        obs_key = self.get_room_key(obs_q, obs_r, 3)
        self._occupied.add((obs_q, obs_r, 3))
        self._room_volumes[obs_key] = []
        self._room_metadata[obs_key] = {
            "room_type": "rotunda",
            "district": "observatory",
            "index_level": 3,
            "volume_count": 0,
        }

    def _collect_cluster_volumes(
        self, room_keys: list[str], max_count: int
    ) -> list[str]:
        """Collect representative volumes from a cluster's data-floor rooms."""
        volumes: list[str] = []
        for rk in room_keys:
            room_vols = self._room_volumes.get(rk, [])
            # Take first few from each room for diversity
            volumes.extend(room_vols[:5])
            if len(volumes) >= max_count:
                break
        return volumes[:max_count]

    # ------------------------------------------------------------------
    # Internal: room metadata
    # ------------------------------------------------------------------

    def _compute_room_metadata(self) -> None:
        """Compute metadata for all data-floor rooms."""
        # Count doors per room based on hex adjacency within cluster patches
        from bfs.resolver.constants import hex_neighbors as _hex_nbrs

        door_counts: dict[str, int] = defaultdict(int)
        for cluster_id, room_keys in self._cluster_rooms.items():
            if cluster_id.startswith("__"):
                continue
            # Build set of coords in this cluster
            coord_set: set[tuple[int, int, int]] = set()
            for rk in room_keys:
                parts = rk.split(":")
                coord_set.add((int(parts[0]), int(parts[1]), int(parts[2])))
            # Count hex-adjacent neighbors within same cluster (capped at 2)
            for rk in room_keys:
                parts = rk.split(":")
                q, r, z = int(parts[0]), int(parts[1]), int(parts[2])
                adj_count = 0
                for nq, nr in _hex_nbrs(q, r):
                    if (nq, nr, z) in coord_set:
                        adj_count += 1
                door_counts[rk] = min(adj_count, 2)  # 2-door constraint

        for room_key, volumes in self._room_volumes.items():
            if room_key in self._room_metadata:
                continue  # already set (index floors)
            parts = room_key.split(":")
            _q, _r, z = int(parts[0]), int(parts[1]), int(parts[2])
            vol_count = len(volumes)
            doors = door_counts.get(room_key, 0)

            # Determine district from first volume's classification
            district = "unknown"
            if volumes:
                cls = self._classifications.get(volumes[0], {})
                district = cls.get("district", "unknown")

            shelves = self._compute_shelves(volumes)

            # Determine dominant type per shelf for engine rendering
            shelf_types: list[str] = []
            for shelf in shelves:
                type_counts: dict[str, int] = {}
                for vid in shelf:
                    ptype = self._classifications.get(vid, {}).get("physical_type", "unknown")
                    type_counts[ptype] = type_counts.get(ptype, 0) + 1
                dominant = max(type_counts, key=type_counts.get) if type_counts else "unknown"
                shelf_types.append(dominant)

            self._room_metadata[room_key] = {
                "room_type": _room_type(vol_count, z, doors),
                "district": district,
                "volume_count": vol_count,
                "door_count": doors,
                "palette": _palette_for_z(z),
                "z_band": z,
                "shelves": shelves,
                "shelf_types": shelf_types,
            }

    def _compute_shelves(self, volumes: list[str]) -> list[list[str]]:
        """Distribute volumes across shelves, grouped by physical type.

        Wall-mountable types (papyrus, wall_inscription, false_door, tomb)
        go on wall shelves. Floor types (stele, scroll, vessel, coffin,
        sculpture, ostrakon, rock) are grouped separately. Each type-group
        occupies contiguous shelf positions so a shelf = one container type.
        """
        if not volumes:
            return []

        # Classify each volume by physical type
        WALL_TYPES = {
            "papyrus", "Papyrus", "writing_tablet", "writing_board",
            "writing_medium", "wall_inscription", "wall", "Wand",
            "false_door", "Scheintür", "Scheintuer", "doorway",
            "tomb", "Grab", "chamber",
            "unknown", "undetermined", "",
        }

        wall_vols: list[str] = []
        floor_groups: dict[str, list[str]] = {}

        for vid in volumes:
            ptype = self._classifications.get(vid, {}).get("physical_type", "unknown")
            if ptype in WALL_TYPES:
                wall_vols.append(vid)
            else:
                # Group by normalized type
                norm = ptype.lower().replace("ä", "a").replace("ü", "u")
                if "stele" in norm or "stel" in norm:
                    key = "stele"
                elif "scroll" in norm or "schriftrolle" in norm:
                    key = "scroll"
                elif "vessel" in norm or "object" in norm or "gefä" in norm or "gefa" in norm:
                    key = "vessel"
                elif "coffin" in norm or "sarg" in norm:
                    key = "coffin"
                elif "sculpt" in norm or "skulptur" in norm:
                    key = "sculpture"
                elif "ostra" in norm:
                    key = "ostrakon"
                elif "rock" in norm or "fels" in norm:
                    key = "rock"
                else:
                    wall_vols.append(vid)  # unknown floor type → treat as wall
                    continue
                floor_groups.setdefault(key, []).append(vid)

        # Build shelves: wall volumes first, then floor groups.
        # Max 10 texts per container (+2 grace if total is 11-12).
        # 24 scrolls → 3 containers of 8. 10 stelae → 1 container of 10.
        MAX_PER_CONTAINER = 10
        GRACE = 2  # allow up to 12 in a single container to avoid a 2-item orphan

        def split_into_containers(vols: list[str]) -> list[list[str]]:
            """Split a volume list into containers of max 10 (+2 grace)."""
            if len(vols) <= MAX_PER_CONTAINER + GRACE:
                return [vols] if vols else []
            # Distribute evenly: ceil(N/10) containers
            n_containers = (len(vols) + MAX_PER_CONTAINER - 1) // MAX_PER_CONTAINER
            per = len(vols) // n_containers
            extra = len(vols) % n_containers
            result = []
            idx = 0
            for i in range(n_containers):
                size = per + (1 if i < extra else 0)
                result.append(vols[idx:idx + size])
                idx += size
            return result

        shelves: list[list[str]] = []

        # Wall containers
        wall_sorted = sorted(wall_vols)
        shelves.extend(split_into_containers(wall_sorted))

        # Floor type containers: split each type group
        for type_key in sorted(floor_groups.keys()):
            group = sorted(floor_groups[type_key])
            shelves.extend(split_into_containers(group))

        # Cap at SHELVES_PER_ROOM (merge overflow into last shelf)
        if len(shelves) > SHELVES_PER_ROOM:
            overflow = []
            for s in shelves[SHELVES_PER_ROOM:]:
                overflow.extend(s)
            shelves = shelves[:SHELVES_PER_ROOM]
            shelves[-1].extend(overflow)

        return shelves


# ------------------------------------------------------------------
# Hex geometry helpers
# ------------------------------------------------------------------

def _hex_ring(cq: int, cr: int, radius: int) -> list[tuple[int, int]]:
    """Return all hex cells at exactly `radius` steps from (cq, cr)."""
    if radius == 0:
        return [(cq, cr)]

    results: list[tuple[int, int]] = []
    # Start at the "top" of the ring and walk around
    q = cq + HEX_DIRECTIONS[4][0] * radius
    r = cr + HEX_DIRECTIONS[4][1] * radius

    for direction_idx in range(6):
        for _ in range(radius):
            results.append((q, r))
            q += HEX_DIRECTIONS[direction_idx][0]
            r += HEX_DIRECTIONS[direction_idx][1]

    return results


def _hex_spiral_coords(
    cq: int, cr: int, max_ring: int
) -> list[tuple[int, int]]:
    """Return hex cells in spiral order from center outward."""
    coords: list[tuple[int, int]] = [(cq, cr)]
    for ring in range(1, max_ring + 1):
        coords.extend(_hex_ring(cq, cr, ring))
    return coords


# ------------------------------------------------------------------
# 2D topology helpers
# ------------------------------------------------------------------

def _min_cluster_distance(
    coords_a: list[tuple[int, int, int]],
    coords_b: list[tuple[int, int, int]],
) -> int:
    """Minimum hex distance between any room pair across two clusters.

    Uses centroids for large clusters to avoid O(N*M) brute force.
    """
    from bfs.resolver.constants import hex_distance as _hex_dist

    # For small clusters, brute-force is fine
    if len(coords_a) * len(coords_b) <= 10000:
        return min(
            _hex_dist((a[0], a[1]), (b[0], b[1]))
            for a in coords_a
            for b in coords_b
        )

    # For large clusters, compare centroids then refine
    cq_a = sum(c[0] for c in coords_a) // len(coords_a)
    cr_a = sum(c[1] for c in coords_a) // len(coords_a)
    cq_b = sum(c[0] for c in coords_b) // len(coords_b)
    cr_b = sum(c[1] for c in coords_b) // len(coords_b)

    # Find rooms closest to the other centroid
    near_a = sorted(coords_a, key=lambda c: _hex_dist((c[0], c[1]), (cq_b, cr_b)))[:5]
    near_b = sorted(coords_b, key=lambda c: _hex_dist((c[0], c[1]), (cq_a, cr_a)))[:5]

    return min(
        _hex_dist((a[0], a[1]), (b[0], b[1]))
        for a in near_a
        for b in near_b
    )


def _closest_pair(
    coords_a: list[tuple[int, int, int]],
    coords_b: list[tuple[int, int, int]],
) -> tuple[tuple[int, int, int], tuple[int, int, int]]:
    """Find the closest room pair between two clusters."""
    from bfs.resolver.constants import hex_distance as _hex_dist

    best_dist = float("inf")
    best_a = coords_a[0]
    best_b = coords_b[0]

    # For large sets, use centroid pre-filtering
    if len(coords_a) * len(coords_b) > 10000:
        cq_b = sum(c[0] for c in coords_b) // len(coords_b)
        cr_b = sum(c[1] for c in coords_b) // len(coords_b)
        coords_a = sorted(
            coords_a, key=lambda c: _hex_dist((c[0], c[1]), (cq_b, cr_b))
        )[:10]
        cq_a = sum(c[0] for c in coords_a) // len(coords_a)
        cr_a = sum(c[1] for c in coords_a) // len(coords_a)
        coords_b = sorted(
            coords_b, key=lambda c: _hex_dist((c[0], c[1]), (cq_a, cr_a))
        )[:10]

    for a in coords_a:
        for b in coords_b:
            d = _hex_dist((a[0], a[1]), (b[0], b[1]))
            if d < best_dist:
                best_dist = d
                best_a = a
                best_b = b

    return best_a, best_b


def _kruskal_mst(
    nodes: list[str],
    edges: list[tuple[str, str, int]],
) -> list[tuple[str, str, int]]:
    """Kruskal's MST over named nodes with weighted edges.

    Returns subset of edges forming the minimum spanning tree.
    Handles disconnected graphs (returns spanning forest).
    """
    # Union-Find
    parent: dict[str, str] = {n: n for n in nodes}
    rank: dict[str, int] = {n: 0 for n in nodes}

    def find(x: str) -> str:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(x: str, y: str) -> bool:
        rx, ry = find(x), find(y)
        if rx == ry:
            return False
        if rank[rx] < rank[ry]:
            rx, ry = ry, rx
        parent[ry] = rx
        if rank[rx] == rank[ry]:
            rank[rx] += 1
        return True

    # Sort edges by weight
    sorted_edges = sorted(edges, key=lambda e: e[2])
    mst: list[tuple[str, str, int]] = []

    for ca, cb, dist in sorted_edges:
        if union(ca, cb):
            mst.append((ca, cb, dist))
            if len(mst) == len(nodes) - 1:
                break

    return mst
