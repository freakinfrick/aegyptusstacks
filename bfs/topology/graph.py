"""L4 Topology Graph — the navigable graph structure of the library."""

from __future__ import annotations

from collections import defaultdict
from typing import Any

import networkx as nx

from bfs.topology.hex_utils import (
    coord_distance,
    direction_name,
    hex_distance,
    hex_neighbors,
    opposite_direction,
)
from bfs.topology.models import (
    DoorEdge,
    RoomDescription,
    ShaftEdge,
    WallEdge,
)


class TopologyGraph:
    """The navigable graph of the library.

    Maintains three edge sets:
    - doors: walkable edges (stored in NetworkX graph for pathfinding)
    - walls: implicit non-walkable edges with proximity scores
    - shafts: vertical connections between z-levels
    """

    def __init__(self) -> None:
        self.doors: dict[tuple[int, int, int], list[DoorEdge]] = defaultdict(list)
        self.walls: dict[tuple[int, int, int], list[WallEdge]] = defaultdict(list)
        self.shafts: dict[tuple[int, int, int], list[ShaftEdge]] = defaultdict(list)
        self._nx_graph: nx.Graph = nx.Graph()
        self._occupied_coords: set[tuple[int, int, int]] = set()
        # Bridge edges added by _repair_connectivity — immune to 2-door constraint
        self._bridge_edges: set[tuple[tuple[int, int, int], tuple[int, int, int]]] = set()

    # ------------------------------------------------------------------
    # Build from resolver output
    # ------------------------------------------------------------------

    def build_from_resolver_state(self, resolver_state: Any) -> None:
        """Build topology from a ResolverState.

        The resolver state is expected to have:
        - room_volumes: dict mapping "q:r:z" -> list of canonical_ids
        - room_metadata: dict mapping "q:r:z" -> metadata dict
        - cluster_rooms: dict mapping cluster_id -> list of (q,r,z) tuples
        """
        # Register occupied coordinates
        if hasattr(resolver_state, "room_volumes"):
            for key in resolver_state.room_volumes:
                if isinstance(key, str):
                    parts = key.split(":")
                    coord = (int(parts[0]), int(parts[1]), int(parts[2]))
                else:
                    coord = tuple(key)
                self._occupied_coords.add(coord)  # type: ignore[arg-type]

        self._build_doors(resolver_state)
        self._build_walls()
        self._build_shafts(resolver_state)
        self._repair_connectivity(resolver_state)
        self._prune_dangling_edges(resolver_state)

    def _build_doors(self, state: Any) -> None:
        """Build door edges from 2D hex-patch adjacency and concordance links.

        Door edges arise from:
        1. Hex-adjacent rooms within same cluster (spanning tree + extras, 2-door max)
        2. Rotunda connections (bridging clusters, no door limit)
        3. Cross-cluster concordance links (texts sharing external IDs)
        """
        import random as _random
        from bfs.resolver.constants import CANONICAL_SEED

        rng = _random.Random(CANONICAL_SEED ^ 0xD00B)
        concordance_door_threshold = 2

        # --- Cluster patch doors (2D hex adjacency) ---
        if hasattr(state, "cluster_rooms"):
            for cluster_id, rooms in state.cluster_rooms.items():
                if cluster_id.startswith("__"):
                    continue  # skip index/rotunda markers

                room_coords: list[tuple[int, int, int]] = []
                for r in rooms:
                    if isinstance(r, str):
                        parts = r.split(":")
                        room_coords.append((int(parts[0]), int(parts[1]), int(parts[2])))
                    else:
                        room_coords.append(tuple(r))  # type: ignore[arg-type]

                if len(room_coords) <= 1:
                    continue

                # Group by z-level (spanning tree per z-slice)
                z_groups: dict[int, list[tuple[int, int, int]]] = defaultdict(list)
                for c in room_coords:
                    z_groups[c[2]].append(c)

                for z, coords in z_groups.items():
                    if len(coords) <= 1:
                        continue
                    coord_set = set(coords)

                    # Find all hex-adjacent pairs within this z-slice
                    edges: list[tuple[tuple[int, int, int], tuple[int, int, int]]] = []
                    for c in coords:
                        for nq, nr in hex_neighbors(c[0], c[1]):
                            neighbor = (nq, nr, z)
                            if neighbor in coord_set and neighbor > c:
                                edges.append((c, neighbor))

                    if not edges:
                        # No hex-adjacent pairs (scattered patch) — connect sequentially
                        for i in range(len(coords) - 1):
                            self.add_door(coords[i], coords[i + 1])
                        continue

                    # Build spanning tree (BFS) for connectivity guarantee
                    tree_edges = self._bfs_spanning_tree(coords, edges)
                    tree_set = set(tree_edges) | {(b, a) for a, b in tree_edges}

                    # Add all tree edges as doors
                    for a, b in tree_edges:
                        self.add_door(a, b)

                    # Track door counts per room
                    door_count: dict[tuple[int, int, int], int] = defaultdict(int)
                    for a, b in tree_edges:
                        door_count[a] += 1
                        door_count[b] += 1

                    # Add extra edges (beyond tree) up to 2-door limit
                    extras = [e for e in edges if e not in tree_set and (e[1], e[0]) not in tree_set]
                    rng.shuffle(extras)
                    for a, b in extras:
                        if door_count[a] < 2 and door_count[b] < 2:
                            self.add_door(a, b)
                            door_count[a] += 1
                            door_count[b] += 1

        # --- Inter-cluster adjacency doors ---
        # Connect rooms from different clusters that are hex-adjacent on same z.
        # This forms continuous walkable regions across cluster boundaries.
        self._build_inter_cluster_doors()

        # --- Rotunda doors (bridge between clusters) ---
        self._build_rotunda_doors(state)

        # --- Concordance doors ---
        if hasattr(state, "concordance_links"):
            for (coord_a, coord_b), shared_refs in state.concordance_links.items():
                if len(shared_refs) >= concordance_door_threshold:
                    self.add_door(coord_a, coord_b)

    def _build_inter_cluster_doors(self) -> None:
        """Connect hex-adjacent rooms across cluster boundaries via spanning tree.

        Uses BFS spanning tree across all same-z occupied rooms to guarantee
        connectivity without opening every possible door. Respects the 2-door
        maximum so rooms retain shelved walls for corpus display.
        """
        from collections import defaultdict, deque

        # Group occupied coords by z-level
        z_groups: dict[int, list[tuple[int, int, int]]] = defaultdict(list)
        for coord in self._occupied_coords:
            z_groups[coord[2]].append(coord)

        for z, coords in z_groups.items():
            coord_set = set(coords)

            # Track door counts (from previously built intra-cluster doors)
            door_count: dict[tuple[int, int, int], int] = defaultdict(int)
            for c in coords:
                door_count[c] = sum(
                    1 for d in self.doors.get(c, []) if d.to_coord[2] == z
                )

            # BFS spanning tree across ALL rooms at this z-level
            visited: set[tuple[int, int, int]] = set()
            for start in coords:
                if start in visited:
                    continue
                visited.add(start)
                queue: deque[tuple[int, int, int]] = deque([start])
                while queue:
                    current = queue.popleft()
                    cq, cr, _ = current
                    for nq, nr in hex_neighbors(cq, cr):
                        neighbor = (nq, nr, z)
                        if neighbor in visited or neighbor not in coord_set:
                            continue
                        visited.add(neighbor)
                        queue.append(neighbor)
                        # Only add door if both rooms have capacity (≤2 same-z doors)
                        has_door = any(
                            d.to_coord == neighbor for d in self.doors.get(current, [])
                        )
                        if has_door:
                            continue
                        if door_count[current] < 2 and door_count[neighbor] < 2:
                            self.add_door(current, neighbor)
                            door_count[current] += 1
                            door_count[neighbor] += 1

    def _bfs_spanning_tree(
        self,
        coords: list[tuple[int, int, int]],
        edges: list[tuple[tuple[int, int, int], tuple[int, int, int]]],
    ) -> list[tuple[tuple[int, int, int], tuple[int, int, int]]]:
        """Build a spanning tree (or forest) via BFS over given edges."""
        from collections import deque

        # Build adjacency list
        adj: dict[tuple[int, int, int], list[tuple[int, int, int]]] = defaultdict(list)
        for a, b in edges:
            adj[a].append(b)
            adj[b].append(a)

        visited: set[tuple[int, int, int]] = set()
        tree: list[tuple[tuple[int, int, int], tuple[int, int, int]]] = []
        queue: deque[tuple[int, int, int]] = deque()

        # BFS from each unvisited node (handles disconnected components)
        for start in coords:
            if start in visited:
                continue
            visited.add(start)
            queue.append(start)
            while queue:
                current = queue.popleft()
                for neighbor in adj[current]:
                    if neighbor not in visited:
                        visited.add(neighbor)
                        tree.append((current, neighbor))
                        queue.append(neighbor)

        return tree

    def _build_rotunda_doors(self, state: Any) -> None:
        """Connect rotunda rooms to their bridged clusters' nearest edge rooms.

        Rotundas are exempt from the 2-door constraint.
        Only creates doors to hex-adjacent rooms (distance 1) so the game engine
        can render physical doorways. If no cluster room is adjacent, relocates
        the rotunda to be adjacent to the nearest target room.
        """
        if not hasattr(state, "rotunda_bridges"):
            return

        rotunda_bridges: dict[str, list[str]] = state.rotunda_bridges
        cluster_rooms_map: dict[str, list[str]] = getattr(state, "cluster_rooms", {})

        for rotunda_key, cluster_ids in rotunda_bridges.items():
            if len(cluster_ids) < 2:
                continue
            parts = rotunda_key.split(":")
            rotunda_coord = (int(parts[0]), int(parts[1]), int(parts[2]))

            # Collect all target rooms from bridged clusters (same z)
            all_targets: list[tuple[int, int, int]] = []
            for cluster_id in cluster_ids:
                rooms = cluster_rooms_map.get(cluster_id, [])
                for rk in rooms:
                    p = rk.split(":")
                    c = (int(p[0]), int(p[1]), int(p[2]))
                    if c[2] == rotunda_coord[2]:
                        all_targets.append(c)

            if not all_targets:
                continue

            # Check if rotunda is adjacent to any target room
            rq, rr, rz = rotunda_coord
            adjacent_targets = [
                t for t in all_targets
                if hex_distance((rq, rr), (t[0], t[1])) == 1
            ]

            # If not adjacent to any target, relocate rotunda next to nearest target
            if not adjacent_targets:
                nearest_target = min(
                    all_targets,
                    key=lambda c: hex_distance((rq, rr), (c[0], c[1])),
                )
                # Find an unoccupied cell adjacent to nearest_target
                new_pos = self._find_adjacent_unoccupied(
                    nearest_target, rz, self._occupied_coords
                )
                if new_pos is None:
                    continue  # can't place — skip this rotunda
                rotunda_coord = new_pos
                rq, rr, rz = rotunda_coord
                self._occupied_coords.add(rotunda_coord)
                # Recompute adjacency
                adjacent_targets = [
                    t for t in all_targets
                    if hex_distance((rq, rr), (t[0], t[1])) == 1
                ]

            # Connect to adjacent rooms from each cluster (up to 2 per cluster)
            for cluster_id in cluster_ids:
                rooms = cluster_rooms_map.get(cluster_id, [])
                if not rooms:
                    continue
                cluster_coords: list[tuple[int, int, int]] = []
                for rk in rooms:
                    p = rk.split(":")
                    c = (int(p[0]), int(p[1]), int(p[2]))
                    if c[2] == rz:
                        cluster_coords.append(c)

                # Only connect to hex-adjacent rooms
                adj_in_cluster = [
                    c for c in cluster_coords
                    if hex_distance((rq, rr), (c[0], c[1])) == 1
                ]

                # Sort: prefer rooms with <2 doors
                adj_in_cluster.sort(
                    key=lambda c: sum(
                        1 for d in self.doors.get(c, []) if d.to_coord[2] == c[2]
                    )
                )

                connected = 0
                for target in adj_in_cluster[:2]:
                    if connected >= 2:
                        break
                    self.add_door(rotunda_coord, target)
                    connected += 1

    @staticmethod
    def _find_adjacent_unoccupied(
        target: tuple[int, int, int],
        z: int,
        occupied: set[tuple[int, int, int]],
    ) -> tuple[int, int, int] | None:
        """Find an unoccupied hex cell adjacent to target at the given z-level."""
        tq, tr, _ = target
        for nq, nr in hex_neighbors(tq, tr):
            if (nq, nr, z) not in occupied:
                return (nq, nr, z)
        return None

    def _build_walls(self) -> None:
        """Build wall edges for hex-adjacent occupied rooms without doors.

        For every occupied room, check all 6 hex neighbors on the same z-level.
        If a neighbor is occupied but has no door connecting them, add a wall edge.
        """
        for coord in self._occupied_coords:
            q, r, z = coord
            for nq, nr in hex_neighbors(q, r):
                neighbor = (nq, nr, z)
                if neighbor not in self._occupied_coords:
                    continue
                # Skip if a door already connects them
                has_door = any(
                    d.to_coord == neighbor for d in self.doors[coord]
                )
                if has_door:
                    continue
                # Avoid duplicate wall edges (only add if coord < neighbor)
                if coord < neighbor:
                    wall = WallEdge(
                        from_coord=coord,
                        to_coord=neighbor,
                        proximity_score=0.0,
                    )
                    self.walls[coord].append(wall)
                    reverse = WallEdge(
                        from_coord=neighbor,
                        to_coord=coord,
                        proximity_score=0.0,
                    )
                    self.walls[neighbor].append(reverse)

    def _build_shafts(self, state: Any) -> None:
        """Build shaft edges — the 4th dimensional semantic axis.

        Shafts connect rooms of the SAME semantic cluster across z-levels.
        This is the temporal/holographic connection: the same concept read
        across different historical periods. Going up or down a shaft means
        "read what earlier/later Egyptians wrote about this same idea."

        Three passes:
        1. Observatory entry: one jump from (0,0,3) to z=0.
        2. Exact alignment: same (q,r) across z-levels (geometric match).
        3. Semantic alignment: same cluster across z-levels, connecting the
           nearest room-pair per z-gap. Minimal (q,r) drift — same district
           region, same topic, just a different era.
        """
        from bfs.resolver.constants import ENTRANCE_COORD

        z_buckets: dict[int, list[tuple[int, int, int]]] = defaultdict(list)
        qr_index: dict[tuple[int, int], list[int]] = defaultdict(list)

        for coord in self._occupied_coords:
            q, r, z = coord
            z_buckets[z].append(coord)
            qr_index[(q, r)].append(z)

        # === Pass 1: Observatory entry ===
        # Connect observatory to the top of the deepest vertical column —
        # the (q,r) position that spans the most z-levels. This becomes
        # the main shaft the player descends through the entire library.
        obs = ENTRANCE_COORD

        # Find the (q,r) with rooms on the most z-levels
        best_column = max(qr_index.items(), key=lambda item: len(item[1]), default=None)
        if best_column:
            col_qr, col_zs = best_column
            top_z = max(z for z in col_zs if z <= 0)
            landing = (col_qr[0], col_qr[1], top_z)
            self.add_shaft(obs, landing)

        # === Pass 2: Exact (q,r) alignment ===
        for (q, r), z_list in qr_index.items():
            if len(z_list) < 2:
                continue
            z_sorted = sorted(z_list, reverse=True)
            for i in range(len(z_sorted) - 1):
                z_upper = z_sorted[i]
                z_lower = z_sorted[i + 1]
                if z_upper > 0 or z_lower > 0:
                    continue
                upper_coord = (q, r, z_upper)
                lower_coord = (q, r, z_lower)
                existing = self.shafts.get(upper_coord, [])
                if not any(s.to_coord == lower_coord for s in existing):
                    self.add_shaft(upper_coord, lower_coord)

        # === Pass 3: Semantic cluster alignment ===
        # For each cluster spanning multiple z-levels, connect rooms across
        # z-gaps by picking the closest room-pair within the same cluster.
        # This creates intentional vertical connections: same topic, different era.
        cluster_rooms = getattr(state, "cluster_rooms", None)
        if cluster_rooms is None and hasattr(state, "__getitem__"):
            cluster_rooms = state.get("cluster_rooms", {})
        if not cluster_rooms:
            cluster_rooms = {}

        for cluster_id, room_keys in cluster_rooms.items():
            # Parse room coords and group by z
            coords_by_z: dict[int, list[tuple[int, int, int]]] = defaultdict(list)
            for rk in room_keys:
                parts = rk.split(":")
                q, r, z = int(parts[0]), int(parts[1]), int(parts[2])
                if z <= 0:
                    coords_by_z[z].append((q, r, z))

            z_levels = sorted(coords_by_z.keys(), reverse=True)
            if len(z_levels) < 2:
                continue

            # Connect each adjacent z-pair within this cluster
            for i in range(len(z_levels) - 1):
                z_upper = z_levels[i]
                z_lower = z_levels[i + 1]

                upper_rooms = coords_by_z[z_upper]
                lower_rooms = coords_by_z[z_lower]

                # Find the closest room-pair between the two z-levels
                best_dist = float("inf")
                best_upper = None
                best_lower = None

                for uc in upper_rooms:
                    for lc in lower_rooms:
                        d = hex_distance((uc[0], uc[1]), (lc[0], lc[1]))
                        if d < best_dist:
                            best_dist = d
                            best_upper = uc
                            best_lower = lc

                # Cap drift: only connect if rooms are within 8 hex distance.
                # Larger gaps mean the cluster is spatially fragmented across
                # z-levels — the shaft would disorient the player.
                MAX_SEMANTIC_DRIFT = 8

                if best_upper and best_lower and best_dist <= MAX_SEMANTIC_DRIFT:
                    existing = self.shafts.get(best_upper, [])
                    if not any(s.to_coord == best_lower for s in existing):
                        self.add_shaft(best_upper, best_lower)

                    # Large clusters: add more shafts (up to 3 per z-pair)
                    if min(len(upper_rooms), len(lower_rooms)) >= 3:
                        extra = 0
                        for uc in upper_rooms:
                            if extra >= 2:
                                break
                            for lc in lower_rooms:
                                if (uc, lc) == (best_upper, best_lower):
                                    continue
                                d = hex_distance((uc[0], uc[1]), (lc[0], lc[1]))
                                if d <= MAX_SEMANTIC_DRIFT:
                                    ex = self.shafts.get(uc, [])
                                    if not any(s.to_coord == lc for s in ex):
                                        self.add_shaft(uc, lc)
                                        extra += 1
                                        break

    def _repair_connectivity(self, resolver_state: Any) -> None:
        """Guarantee 1 connected component per z-level on data floors.

        Instead of long-range bridge doors, inserts CORRIDOR ROOMS at
        intermediate hex coordinates between disconnected components.
        Corridor rooms are empty (volume_count=0, room_type="corridor")
        and chain-connected with normal hex-adjacent doors. The game
        engine stamps them as regular rooms — no special rendering needed.
        """
        from collections import deque

        data_coords = {c for c in self._occupied_coords if c[2] <= 0}

        # Phase 1: Register orphan rooms
        nx_nodes = set(self._nx_graph.nodes())
        orphans = data_coords - nx_nodes
        for orphan in orphans:
            self._nx_graph.add_node(orphan)

        # Phase 2: Per z-level, merge components via corridor room chains
        z_groups: dict[int, list[tuple[int, int, int]]] = defaultdict(list)
        for c in data_coords:
            z_groups[c[2]].append(c)

        for z, coords in z_groups.items():
            if len(coords) <= 1:
                continue

            z_set = set(coords)
            visited: set[tuple[int, int, int]] = set()
            components: list[list[tuple[int, int, int]]] = []

            for start in coords:
                if start in visited:
                    continue
                comp: list[tuple[int, int, int]] = []
                queue: deque[tuple[int, int, int]] = deque([start])
                while queue:
                    cur = queue.popleft()
                    if cur in visited:
                        continue
                    visited.add(cur)
                    comp.append(cur)
                    for door in self.doors.get(cur, []):
                        tc = tuple(door.to_coord)
                        if tc[2] == z and tc in z_set and tc not in visited:
                            queue.append(tc)
                components.append(comp)

            if len(components) <= 1:
                continue

            # MST over components
            edges: list[tuple[int, int, int, tuple, tuple]] = []
            for ci in range(len(components)):
                for cj in range(ci + 1, len(components)):
                    sample_a = components[ci][:30]
                    sample_b = components[cj][:30]
                    best_d = float("inf")
                    best_a, best_b = sample_a[0], sample_b[0]
                    for a in sample_a:
                        for b in sample_b:
                            d = hex_distance((a[0], a[1]), (b[0], b[1]))
                            if d < best_d:
                                best_d = d
                                best_a, best_b = a, b
                    edges.append((best_d, ci, cj, best_a, best_b))

            edges.sort()
            parent = list(range(len(components)))
            def find(x: int) -> int:
                while parent[x] != x:
                    parent[x] = parent[parent[x]]
                    x = parent[x]
                return x
            def union(x: int, y: int) -> bool:
                rx, ry = find(x), find(y)
                if rx == ry:
                    return False
                parent[ry] = rx
                return True

            for dist, ci, cj, room_a, room_b in edges:
                if union(ci, cj):
                    if dist <= 1:
                        # Already adjacent — just add a door
                        self.add_door(room_a, room_b)
                        self._bridge_edges.add((room_a, room_b))
                        self._bridge_edges.add((room_b, room_a))
                    else:
                        # Insert corridor rooms along hex path
                        self._insert_corridor_chain(
                            room_a, room_b, z, resolver_state
                        )

    def _insert_corridor_chain(
        self,
        room_a: tuple[int, int, int],
        room_b: tuple[int, int, int],
        z: int,
        resolver_state: Any,
    ) -> None:
        """Insert corridor rooms along the hex path from room_a to room_b.

        Each corridor room is an empty room at an intermediate BFS coordinate.
        Rooms are chained with hex-adjacent doors: A → C1 → C2 → ... → B.
        Corridor rooms are added to the resolver state so the server serves them.
        """
        # Build hex path from A to B using greedy steps in BFS coords
        path = _greedy_hex_path_2d(
            (room_a[0], room_a[1]),
            (room_b[0], room_b[1]),
        )

        # path[0] = room_a's (q,r), path[-1] = room_b's (q,r)
        # Intermediate positions are path[1:-1]
        if len(path) < 3:
            # Adjacent or 1 step — just add a door
            self.add_door(room_a, room_b)
            self._bridge_edges.add((room_a, room_b))
            self._bridge_edges.add((room_b, room_a))
            return

        # Get room_metadata and room_volumes dicts from resolver state
        room_meta = resolver_state.room_metadata if hasattr(resolver_state, "room_metadata") else resolver_state.get("room_metadata", {})
        room_vols = resolver_state.room_volumes if hasattr(resolver_state, "room_volumes") else resolver_state.get("room_volumes", {})

        # Determine corridor district from endpoint rooms
        a_key = f"{room_a[0]}:{room_a[1]}:{room_a[2]}"
        district = room_meta.get(a_key, {}).get("district", "unknown")

        # Insert corridor rooms at intermediate coordinates
        chain: list[tuple[int, int, int]] = [room_a]
        for q, r in path[1:-1]:
            coord = (q, r, z)
            room_key_check = f"{q}:{r}:{z}"
            if room_key_check in room_meta:
                # Position has a real room (not just an _occupied_coords ghost
                # from add_door) — include it in the chain
                chain.append(coord)
                continue

            # Create corridor room
            room_key = f"{q}:{r}:{z}"
            self._occupied_coords.add(coord)

            # Add to resolver state
            room_vols[room_key] = []
            room_meta[room_key] = {
                "room_type": "corridor",
                "district": district,
                "volume_count": 0,
                "door_count": 2,
                "palette": room_meta.get(a_key, {}).get("palette", "warm_amber"),
                "z_band": z,
                "shelves": [],
                "shelf_types": [],
            }
            chain.append(coord)

        chain.append(room_b)

        # Connect chain with adjacent doors
        for i in range(len(chain) - 1):
            c_from = chain[i]
            c_to = chain[i + 1]
            # Only add if not already connected
            existing = self.doors.get(c_from, [])
            if not any(d.to_coord == c_to for d in existing):
                self.add_door(c_from, c_to)
                self._bridge_edges.add((c_from, c_to))
                self._bridge_edges.add((c_to, c_from))

    def _prune_dangling_edges(self, resolver_state: Any) -> None:
        """Remove door/shaft edges whose target has no room_metadata entry.

        add_door registers both endpoints in _occupied_coords, so a later
        corridor chain can treat a ghost coord as an existing room. This
        pass strips any edge whose to_coord was never materialized as a
        real room — preventing doors-to-nowhere in the served data.
        """
        room_meta = (
            resolver_state.room_metadata
            if hasattr(resolver_state, "room_metadata")
            else resolver_state.get("room_metadata", {})
        )

        def _coord_key(c: tuple[int, int, int]) -> str:
            return f"{c[0]}:{c[1]}:{c[2]}"

        pruned = 0
        for coord in list(self.doors.keys()):
            orig = self.doors[coord]
            filtered = [d for d in orig if _coord_key(tuple(d.to_coord)) in room_meta]
            if len(filtered) < len(orig):
                pruned += len(orig) - len(filtered)
                self.doors[coord] = filtered

        if pruned:
            import sys
            print(f"[topology] pruned {pruned} dangling door edge(s)", file=sys.stderr)

    # ------------------------------------------------------------------
    # Edge mutation
    # ------------------------------------------------------------------

    def add_door(
        self,
        from_coord: tuple[int, int, int],
        to_coord: tuple[int, int, int],
        direction: str | None = None,
    ) -> None:
        """Add a bidirectional door edge."""
        self._occupied_coords.add(from_coord)
        self._occupied_coords.add(to_coord)

        if direction is None:
            dq = to_coord[0] - from_coord[0]
            dr = to_coord[1] - from_coord[1]
            if from_coord[2] != to_coord[2]:
                direction = "UP" if to_coord[2] > from_coord[2] else "DOWN"
            else:
                direction = direction_name(dq, dr)

        opp = opposite_direction(direction)

        self.doors[from_coord].append(
            DoorEdge(from_coord=from_coord, to_coord=to_coord, direction=direction)
        )
        self.doors[to_coord].append(
            DoorEdge(from_coord=to_coord, to_coord=from_coord, direction=opp)
        )
        self._nx_graph.add_edge(from_coord, to_coord, weight=1.0, type="door")

    def add_shaft(
        self,
        from_coord: tuple[int, int, int],
        to_coord: tuple[int, int, int],
    ) -> None:
        """Add a vertical shaft connection."""
        self._occupied_coords.add(from_coord)
        self._occupied_coords.add(to_coord)

        up_dir = "UP" if to_coord[2] > from_coord[2] else "DOWN"
        down_dir = opposite_direction(up_dir)

        self.shafts[from_coord].append(
            ShaftEdge(from_coord=from_coord, to_coord=to_coord, direction=up_dir)
        )
        self.shafts[to_coord].append(
            ShaftEdge(from_coord=to_coord, to_coord=from_coord, direction=down_dir)
        )
        # Shafts are also walkable — add to NetworkX graph with higher weight
        self._nx_graph.add_edge(from_coord, to_coord, weight=2.0, type="shaft")

    # ------------------------------------------------------------------
    # Queries
    # ------------------------------------------------------------------

    def neighbors(self, coord: tuple[int, int, int]) -> list[tuple[int, int, int]]:
        """All door-connected rooms."""
        return [d.to_coord for d in self.doors.get(coord, [])]

    def pathfind(
        self, start: tuple[int, int, int], end: tuple[int, int, int]
    ) -> list[tuple[int, int, int]]:
        """A* shortest path through door (and shaft) edges."""

        def heuristic(
            a: tuple[int, int, int], b: tuple[int, int, int]
        ) -> float:
            return coord_distance(a, b)

        try:
            path: list[tuple[int, int, int]] = nx.astar_path(
                self._nx_graph, start, end, heuristic
            )
            return path
        except (nx.NetworkXNoPath, nx.NodeNotFound):
            return []

    def wall_neighbors(self, coord: tuple[int, int, int]) -> list[WallEdge]:
        """Get wall edges for a room."""
        return self.walls.get(coord, [])

    def describe_room(self, coord: tuple[int, int, int]) -> RoomDescription:
        """Full description of a room's connections."""
        return RoomDescription(
            coord=coord,
            doors=list(self.doors.get(coord, [])),
            walls=list(self.walls.get(coord, [])),
            shafts=list(self.shafts.get(coord, [])),
        )

    def get_door_count(self, coord: tuple[int, int, int]) -> int:
        """Number of door edges for a room."""
        return len(self.doors.get(coord, []))

    def enforce_two_door_constraint(self) -> int:
        """Remove excess horizontal doors. Each room keeps max 2 horizontal doors + shafts.

        Bridge edges (created by _repair_connectivity for z-level connectivity)
        are immune — never removed. Returns number of doors removed.
        """
        removed = 0
        for coord in list(self.doors.keys()):
            doors = self.doors[coord]
            horizontal = [d for d in doors if d.direction not in ("UP", "DOWN")]
            vertical = [d for d in doors if d.direction in ("UP", "DOWN")]

            if len(horizontal) <= 2:
                continue

            # Partition into bridge (protected) and non-bridge (removable)
            bridge_doors = [
                d for d in horizontal
                if (tuple(d.from_coord), tuple(d.to_coord)) in self._bridge_edges
            ]
            non_bridge = [
                d for d in horizontal
                if (tuple(d.from_coord), tuple(d.to_coord)) not in self._bridge_edges
            ]

            # Keep all bridges + up to 2 non-bridge doors
            max_non_bridge = max(0, 2 - len(bridge_doors))
            kept_non_bridge = non_bridge[:max_non_bridge]
            to_remove = non_bridge[max_non_bridge:]

            self.doors[coord] = bridge_doors + kept_non_bridge + vertical
            removed += len(to_remove)

            # Remove from NetworkX and reverse door lists
            for door in to_remove:
                tc = tuple(door.to_coord)
                if self._nx_graph.has_edge(coord, tc):
                    self._nx_graph.remove_edge(coord, tc)
                # Remove the reverse door
                reverse_doors = self.doors.get(tc, [])
                self.doors[tc] = [
                    d for d in reverse_doors
                    if tuple(d.to_coord) != coord
                ]

        return removed

    # ------------------------------------------------------------------
    # Stats
    # ------------------------------------------------------------------

    def stats(self) -> dict[str, int]:
        """Graph statistics."""
        return {
            "total_rooms": len(self._occupied_coords),
            "total_doors": sum(len(d) for d in self.doors.values()) // 2,
            "total_walls": sum(len(w) for w in self.walls.values()) // 2,
            "total_shafts": sum(len(s) for s in self.shafts.values()) // 2,
            "connected_components": nx.number_connected_components(self._nx_graph),
        }


def _greedy_hex_path_2d(
    start: tuple[int, int],
    end: tuple[int, int],
) -> list[tuple[int, int]]:
    """Build a hex path from start to end using greedy steps in 2D BFS coords.

    At each step, move to the hex neighbor closest to the destination.
    Returns list of (q, r) positions including start and end.
    """
    from bfs.topology.hex_utils import hex_neighbors as _hex_nbrs

    path = [start]
    current = start
    max_steps = hex_distance(start, end) + 5
    for _ in range(max_steps):
        if current == end:
            break
        best_nbr = current
        best_dist = hex_distance(current, end)
        for nq, nr in _hex_nbrs(current[0], current[1]):
            d = hex_distance((nq, nr), end)
            if d < best_dist:
                best_dist = d
                best_nbr = (nq, nr)
        if best_nbr == current:
            break  # stuck — shouldn't happen on hex grid
        current = best_nbr
        path.append(current)
    return path
