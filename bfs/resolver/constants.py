"""BFS Spatial Resolver constants."""

CANONICAL_SEED: int = 0xAE60001
MAX_VOLUMES_PER_ROOM: int = 50
SHELVES_PER_ROOM: int = 12   # one per type group — no artificial cap
VOLUMES_PER_SHELF: int = 50  # each shelf = one type container, holds all texts of that type
ENTRANCE_COORD: tuple[int, int, int] = (0, 0, 3)  # Observatory
WORKSPACE_COORD: tuple[int, int, int] = (999, 999, 0)

# 2D topology constants
ROTUNDA_MAX_DOORS: int = 6        # rotundas can have up to 6 doors
PATCH_MIN_RADIUS: int = 1         # minimum hex-disk radius
CROSSROADS_INTERVAL: int = 20     # 1 internal junction per N rooms in large patches
CLUSTER_BRIDGE_DISTANCE: int = 4  # max hex-distance to bridge clusters with rotunda

# Hex axial directions: E, NE, NW, W, SW, SE
HEX_DIRECTIONS: list[tuple[int, int]] = [
    (1, 0),    # E
    (1, -1),   # NE
    (0, -1),   # NW
    (-1, 0),   # W
    (-1, 1),   # SW
    (0, 1),    # SE
]


def hex_neighbors(q: int, r: int) -> list[tuple[int, int]]:
    """Return the 6 hex-adjacent coordinates in axial coords."""
    return [(q + dq, r + dr) for dq, dr in HEX_DIRECTIONS]


def hex_distance(a: tuple[int, int], b: tuple[int, int]) -> int:
    """Hex distance between two axial coordinates."""
    dq = abs(a[0] - b[0])
    dr = abs(a[1] - b[1])
    ds = abs((-a[0] - a[1]) - (-b[0] - b[1]))
    return max(dq, dr, ds)
