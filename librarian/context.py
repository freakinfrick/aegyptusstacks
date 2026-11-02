"""Context builder for the Librarian — injects location awareness (~200 tokens)."""

from __future__ import annotations

from typing import Any


def build_context(session: dict[str, Any], room: dict[str, Any]) -> str:
    """Build a compact context header for the Librarian.

    Target: ~200 tokens.  Includes position, shelves, doors, shafts,
    workspace contents, and session statistics.
    """
    lines: list[str] = []

    # Location
    district = room.get("district", "unknown")
    room_type = room.get("room_type", "chamber")
    z = room.get("z", 0)
    room_name = room.get("name", "")
    loc = f"[LOCATION: {district} / {room_type} / z={z}"
    if room_name:
        loc += f' / room "{room_name}"'
    loc += "]"
    lines.append(loc)

    # Shelves — first 4, with volume counts
    shelves = room.get("shelves", [])
    vol_count = room.get("volume_count", 0)
    if shelves:
        shelf_parts = []
        for s in shelves[:4]:
            label = s.get("label", f"Shelf {s.get('index', '?')}")
            vols = len(s.get("volumes", []))
            shelf_parts.append(f'{label}: {vols} vols')
        lines.append(
            f"[SHELVES: {len(shelves)} shelves, {vol_count} volumes. {', '.join(shelf_parts)}]"
        )
    else:
        lines.append("[SHELVES: empty]")

    # Doors — horizontal exits
    doors = room.get("door_edges", [])
    if doors:
        door_parts = []
        for d in doors[:4]:
            direction = d.get("direction", "?")
            dest = d.get("destination_name", "")
            entry = f"{direction}"
            if dest:
                entry += f' → "{dest}"'
            door_parts.append(entry)
        lines.append(f"[DOORS: {', '.join(door_parts)}]")

    # Shafts — vertical connections
    shafts = room.get("shaft_edges", [])
    if shafts:
        shaft_parts = []
        for s in shafts:
            direction = s.get("direction", "?")
            dest = s.get("destination_name", "")
            entry = f"{direction}"
            if dest:
                entry += f' → "{dest}"'
            shaft_parts.append(entry)
        lines.append(f"[SHAFTS: {', '.join(shaft_parts)}]")

    # Workspace
    workspace = session.get("workspace_volumes", [])
    lines.append(f"[WORKSPACE: {len(workspace)} gathered volumes]")

    # Session stats
    visited = len(session.get("rooms_visited", []))
    interaction = session.get("interaction_count", 0)
    lines.append(f"[SESSION: {visited} rooms visited, interaction {interaction}]")

    return "\n".join(lines)
