"""Hex grid utilities for axial coordinate system."""

from __future__ import annotations

# Six axial directions for flat-top hexagons
HEX_DIRECTIONS: list[tuple[int, int]] = [
    (1, 0),   # E
    (1, -1),  # NE
    (0, -1),  # NW
    (-1, 0),  # W
    (-1, 1),  # SW
    (0, 1),   # SE
]

DIRECTION_NAMES: list[str] = ["E", "NE", "NW", "W", "SW", "SE"]

_OPPOSITE: dict[str, str] = {
    "E": "W", "W": "E",
    "NE": "SW", "SW": "NE",
    "NW": "SE", "SE": "NW",
    "UP": "DOWN", "DOWN": "UP",
}


def hex_neighbors(q: int, r: int) -> list[tuple[int, int]]:
    """Return the 6 hex-adjacent coordinates."""
    return [(q + dq, r + dr) for dq, dr in HEX_DIRECTIONS]


def hex_distance(a: tuple[int, int], b: tuple[int, int]) -> int:
    """Hex distance using cube coordinate conversion.

    Convert axial (q, r) to cube (x, y, z) where x=q, z=r, y=-x-z.
    Distance = max(|dx|, |dy|, |dz|).
    """
    ax, az = a
    ay = -ax - az
    bx, bz = b
    by = -bx - bz
    return max(abs(ax - bx), abs(ay - by), abs(az - bz))


def coord_distance(a: tuple[int, int, int], b: tuple[int, int, int]) -> float:
    """3D distance: hex distance on q/r plane + vertical distance on z.

    Vertical movement costs 2x horizontal.
    """
    h = hex_distance((a[0], a[1]), (b[0], b[1]))
    v = abs(a[2] - b[2])
    return float(h + v * 2)


def direction_name(dq: int, dr: int) -> str:
    """Map a (dq, dr) offset to a direction name.

    For hex-adjacent offsets, returns the exact direction.
    For longer offsets (bridge doors), returns the closest cardinal direction
    based on the dominant axis of the offset vector.
    """
    try:
        idx = HEX_DIRECTIONS.index((dq, dr))
        return DIRECTION_NAMES[idx]
    except ValueError:
        if dq == 0 and dr == 0:
            return "?"
        # For non-adjacent: find closest hex direction by angle similarity
        # In axial coords, convert to cube (x=dq, z=dr, y=-dq-dr) and pick
        # the direction whose unit vector is most aligned.
        best_idx = 0
        best_dot = float("-inf")
        for i, (hq, hr) in enumerate(HEX_DIRECTIONS):
            dot = dq * hq + dr * hr
            if dot > best_dot:
                best_dot = dot
                best_idx = i
        return DIRECTION_NAMES[best_idx]


def opposite_direction(direction: str) -> str:
    """Return the opposite direction (E -> W, NE -> SW, etc.)."""
    return _OPPOSITE.get(direction, "?")
