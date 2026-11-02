"""Tests for L4 Topology Graph."""

from bfs.topology.graph import TopologyGraph
from bfs.topology.hex_utils import (
    coord_distance,
    direction_name,
    hex_distance,
    hex_neighbors,
    opposite_direction,
)


# ------------------------------------------------------------------
# hex_utils tests
# ------------------------------------------------------------------


def test_hex_neighbors() -> None:
    neighbors = hex_neighbors(0, 0)
    assert len(neighbors) == 6
    assert (1, 0) in neighbors
    assert (-1, 0) in neighbors
    assert (0, 1) in neighbors
    assert (0, -1) in neighbors
    assert (1, -1) in neighbors
    assert (-1, 1) in neighbors


def test_hex_neighbors_offset() -> None:
    neighbors = hex_neighbors(3, -2)
    assert len(neighbors) == 6
    assert (4, -2) in neighbors  # E
    assert (4, -3) in neighbors  # NE
    assert (3, -3) in neighbors  # NW
    assert (2, -2) in neighbors  # W
    assert (2, -1) in neighbors  # SW
    assert (3, -1) in neighbors  # SE


def test_hex_distance_zero() -> None:
    assert hex_distance((0, 0), (0, 0)) == 0


def test_hex_distance_adjacent() -> None:
    assert hex_distance((0, 0), (1, 0)) == 1
    assert hex_distance((0, 0), (0, 1)) == 1
    assert hex_distance((0, 0), (-1, 1)) == 1


def test_hex_distance_two() -> None:
    assert hex_distance((0, 0), (2, -1)) == 2
    assert hex_distance((0, 0), (2, 0)) == 2


def test_hex_distance_symmetric() -> None:
    assert hex_distance((3, -2), (0, 0)) == hex_distance((0, 0), (3, -2))


def test_hex_distance_larger() -> None:
    # (0,0) -> (3,-3): cube coords (0,0,0) -> (3,0,-3), dist=3
    assert hex_distance((0, 0), (3, -3)) == 3


def test_coord_distance_horizontal() -> None:
    assert coord_distance((0, 0, 0), (1, 0, 0)) == 1.0


def test_coord_distance_vertical() -> None:
    # Vertical costs 2x
    assert coord_distance((0, 0, 0), (0, 0, 1)) == 2.0
    assert coord_distance((0, 0, 0), (0, 0, -3)) == 6.0


def test_coord_distance_mixed() -> None:
    # 1 hex step + 1 vertical step = 1 + 2 = 3
    assert coord_distance((0, 0, 0), (1, 0, 1)) == 3.0


def test_direction_name() -> None:
    assert direction_name(1, 0) == "E"
    assert direction_name(-1, 0) == "W"
    assert direction_name(1, -1) == "NE"
    assert direction_name(0, 1) == "SE"


def test_direction_name_unknown() -> None:
    assert direction_name(2, 0) == "?"


def test_opposite_direction() -> None:
    assert opposite_direction("E") == "W"
    assert opposite_direction("W") == "E"
    assert opposite_direction("NE") == "SW"
    assert opposite_direction("SW") == "NE"
    assert opposite_direction("NW") == "SE"
    assert opposite_direction("SE") == "NW"
    assert opposite_direction("UP") == "DOWN"
    assert opposite_direction("DOWN") == "UP"


# ------------------------------------------------------------------
# TopologyGraph tests
# ------------------------------------------------------------------


def test_add_door_bidirectional() -> None:
    graph = TopologyGraph()
    graph._occupied_coords.add((0, 0, 0))
    graph._occupied_coords.add((1, 0, 0))
    graph.add_door((0, 0, 0), (1, 0, 0))
    assert len(graph.doors[(0, 0, 0)]) == 1
    assert len(graph.doors[(1, 0, 0)]) == 1
    assert graph.doors[(0, 0, 0)][0].direction == "E"
    assert graph.doors[(1, 0, 0)][0].direction == "W"


def test_add_door_vertical() -> None:
    graph = TopologyGraph()
    graph.add_door((0, 0, 0), (0, 0, 1))
    assert graph.doors[(0, 0, 0)][0].direction == "UP"
    assert graph.doors[(0, 0, 1)][0].direction == "DOWN"


def test_add_shaft() -> None:
    graph = TopologyGraph()
    graph.add_shaft((5, 3, 0), (5, 3, 1))
    assert len(graph.shafts[(5, 3, 0)]) == 1
    assert len(graph.shafts[(5, 3, 1)]) == 1
    assert graph.shafts[(5, 3, 0)][0].direction == "UP"
    assert graph.shafts[(5, 3, 1)][0].direction == "DOWN"
    # Shafts are walkable
    assert graph._nx_graph.has_edge((5, 3, 0), (5, 3, 1))


def test_neighbors() -> None:
    graph = TopologyGraph()
    graph.add_door((0, 0, 0), (1, 0, 0))
    graph.add_door((0, 0, 0), (-1, 0, 0))
    result = graph.neighbors((0, 0, 0))
    assert (1, 0, 0) in result
    assert (-1, 0, 0) in result
    assert len(result) == 2


def test_pathfinding() -> None:
    graph = TopologyGraph()
    for i in range(5):
        graph._occupied_coords.add((i, 0, 0))
    for i in range(4):
        graph.add_door((i, 0, 0), (i + 1, 0, 0))
    path = graph.pathfind((0, 0, 0), (4, 0, 0))
    assert len(path) == 5
    assert path[0] == (0, 0, 0)
    assert path[-1] == (4, 0, 0)


def test_pathfinding_no_path() -> None:
    graph = TopologyGraph()
    graph._occupied_coords.add((0, 0, 0))
    graph._occupied_coords.add((10, 10, 0))
    path = graph.pathfind((0, 0, 0), (10, 10, 0))
    assert path == []


def test_pathfinding_with_shaft() -> None:
    graph = TopologyGraph()
    graph.add_door((0, 0, 0), (1, 0, 0))
    graph.add_shaft((1, 0, 0), (1, 0, 1))
    graph.add_door((1, 0, 1), (2, 0, 1))
    path = graph.pathfind((0, 0, 0), (2, 0, 1))
    assert len(path) == 4
    assert path == [(0, 0, 0), (1, 0, 0), (1, 0, 1), (2, 0, 1)]


def test_wall_construction() -> None:
    graph = TopologyGraph()
    # Two adjacent rooms with no door between them
    graph._occupied_coords.add((0, 0, 0))
    graph._occupied_coords.add((1, 0, 0))
    graph._build_walls()
    assert len(graph.wall_neighbors((0, 0, 0))) == 1
    assert len(graph.wall_neighbors((1, 0, 0))) == 1
    assert graph.wall_neighbors((0, 0, 0))[0].to_coord == (1, 0, 0)


def test_wall_not_created_where_door_exists() -> None:
    graph = TopologyGraph()
    graph._occupied_coords.add((0, 0, 0))
    graph._occupied_coords.add((1, 0, 0))
    graph.add_door((0, 0, 0), (1, 0, 0))
    graph._build_walls()
    assert len(graph.wall_neighbors((0, 0, 0))) == 0


def test_describe_room() -> None:
    graph = TopologyGraph()
    graph.add_door((0, 0, 0), (1, 0, 0))
    graph.add_shaft((0, 0, 0), (0, 0, -1))
    desc = graph.describe_room((0, 0, 0))
    assert desc.coord == (0, 0, 0)
    assert len(desc.doors) == 1
    assert len(desc.shafts) == 1


def test_stats() -> None:
    graph = TopologyGraph()
    graph.add_door((0, 0, 0), (1, 0, 0))
    graph.add_door((1, 0, 0), (2, 0, 0))
    graph.add_shaft((0, 0, 0), (0, 0, 1))
    s = graph.stats()
    assert s["total_rooms"] == 4  # (0,0,0), (1,0,0), (2,0,0), (0,0,1)
    assert s["total_doors"] == 2
    assert s["total_shafts"] == 1
    assert s["connected_components"] == 1


def test_enforce_two_door_constraint() -> None:
    graph = TopologyGraph()
    center = (0, 0, 0)
    graph._occupied_coords.add(center)
    # Give center 3 horizontal doors
    for dq, dr in [(1, 0), (0, 1), (-1, 0)]:
        neighbor = (dq, dr, 0)
        graph._occupied_coords.add(neighbor)
        graph.add_door(center, neighbor)
    assert graph.get_door_count(center) == 3
    removed = graph.enforce_two_door_constraint()
    assert removed > 0
    # Center should now have at most 2 horizontal doors
    horizontal = [
        d for d in graph.doors[center] if d.direction not in ("UP", "DOWN")
    ]
    assert len(horizontal) <= 2


def test_get_door_count() -> None:
    graph = TopologyGraph()
    assert graph.get_door_count((0, 0, 0)) == 0
    graph.add_door((0, 0, 0), (1, 0, 0))
    assert graph.get_door_count((0, 0, 0)) == 1
