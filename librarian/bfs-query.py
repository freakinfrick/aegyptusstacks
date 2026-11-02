#!/usr/bin/env python3
"""Minimal BFS JSON-RPC client — thin wrapper for stacks-librarian.sh.
Connects to BFS server at localhost:8081 via HTTP POST /rpc.
"""
import json, sys, os
try:
    import httpx
except ImportError:
    import urllib.request
    def _post(url, data):
        req = urllib.request.Request(url, data=json.dumps(data).encode(),
                                     headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=10) as r:
            return json.loads(r.read())
    httpx = None

BFS_URL = os.environ.get("BFS_URL", "http://localhost:8081/rpc")

# Face directions in hex order (matches Rust world gen wall indices 0-5)
FACE_NAMES = ["East", "Northeast", "Northwest", "West", "Southwest", "Southeast"]


def rpc(method, params):
    payload = {"jsonrpc": "2.0", "method": method, "params": params, "id": 1}
    if httpx:
        r = httpx.post(BFS_URL, json=payload, timeout=10)
        return r.json()
    return _post(BFS_URL, payload)


def _classify_shelf_type(stype: str):
    """Classify a BFS shelf_type into fixture kind + label.

    Mirrors the Rust world gen logic in stamp_wall_fixtures / stamp_floor_fixtures.
    Returns (placement, label) where placement is "wall" or "floor".
    """
    s = stype.lower()
    # Wall types (checked first, same order as Rust)
    if s in ("", "unknown") or "papyrus" in s or "writing" in s:
        return ("wall", "Papyrus Shelf")
    if "wand" in s or "wall" in s or "inscription" in s:
        return ("wall", "Wall Inscription")
    if "scheint" in s or "false_door" in s or "doorway" in s:
        return ("wall", "False Door")
    if "grab" in s or "tomb" in s or "chamber" in s:
        return ("wall", "Tomb Alcove")
    # Floor types
    if "stele" in s or "stel" in s:
        return ("floor", "Stele")
    if "scroll" in s or "schriftrolle" in s:
        return ("floor", "Scroll Rack")
    if "vessel" in s or "object" in s or "gef" in s:
        return ("floor", "Vessel")
    if "sculpt" in s or "skulptur" in s:
        return ("floor", "Sculpture")
    if "coffin" in s or "sarg" in s:
        return ("floor", "Coffin")
    if "ostra" in s:
        return ("floor", "Ostrakon")
    if "rock" in s or "fels" in s:
        return ("floor", "Rock Inscription")
    # Fallback
    return ("wall", "Shelf")


def _dir_to_wall_index(d: str) -> int | None:
    return {"E": 0, "NE": 1, "NW": 2, "W": 3, "SW": 4, "SE": 5}.get(d.upper())


def locate_fixtures(room_data: dict) -> dict:
    """Build fixture map from BFS room data.

    Returns a compact dict the librarian can use to describe where texts are:
    { "fixtures": [ { "shelf_index", "label", "placement", "face", "volumes" } ] }
    """
    meta = room_data.get("result", room_data)

    shelf_types = meta.get("shelf_types", [])
    shelves = meta.get("shelves", [])
    door_edges = meta.get("door_edges", [])

    # Determine which wall faces are doors (not available for shelves)
    door_faces = set()
    for edge in door_edges:
        d = edge.get("direction", "")
        idx = _dir_to_wall_index(d)
        if idx is not None:
            door_faces.add(idx)

    # Non-door faces in order — wall fixtures are assigned to these
    non_door_faces = [i for i in range(6) if i not in door_faces]

    # Classify each shelf
    wall_fixtures = []
    floor_fixtures = []
    for shelf_idx, stype in enumerate(shelf_types):
        placement, label = _classify_shelf_type(stype)
        vols = shelves[shelf_idx] if shelf_idx < len(shelves) else []
        entry = {
            "shelf_index": shelf_idx,
            "label": label,
            "placement": placement,
            "volumes": vols,
        }
        if placement == "wall":
            wall_fixtures.append(entry)
        else:
            floor_fixtures.append(entry)

    # Assign wall faces to wall fixtures (first N non-door faces, in order)
    for i, fix in enumerate(wall_fixtures):
        if i < len(non_door_faces):
            fix["face"] = FACE_NAMES[non_door_faces[i]]
        else:
            fix["face"] = "wall"

    # Floor fixtures don't have a specific face
    for fix in floor_fixtures:
        fix["face"] = "room interior"

    return {"fixtures": wall_fixtures + floor_fixtures}


def _fix_negative_coords(argv):
    """Insert '--' before negative-looking coordinates so argparse doesn't
    treat them as option flags."""
    if "--" in argv:
        return argv
    fixed = list(argv)
    for i, arg in enumerate(fixed):
        if i > 0 and arg.startswith("-") and "," in arg:
            fixed.insert(i, "--")
            break
    return fixed


def main():
    import argparse
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd")

    s = sub.add_parser("search")
    s.add_argument("query")
    s.add_argument("--limit", type=int, default=10)
    s.add_argument("--json", action="store_true", default=True)

    r = sub.add_parser("room")
    r.add_argument("coord", help="Q,R,Z")

    rd = sub.add_parser("read")
    rd.add_argument("canonical_id")

    rt = sub.add_parser("room-texts")
    rt.add_argument("coord", help="Q,R,Z")

    d = sub.add_parser("district")
    d.add_argument("name")

    loc = sub.add_parser("locate")
    loc.add_argument("coord", help="Q,R,Z")
    loc.add_argument("--volume", help="Filter to a specific volume ID", default=None)

    w = sub.add_parser("where")
    w.add_argument("text", help="Canonical ID or search query")

    sub.add_parser("status")

    args = p.parse_args(_fix_negative_coords(sys.argv[1:]))
    if not args.cmd:
        p.print_help()
        return

    try:
        if args.cmd == "search":
            result = rpc("search", {"query": args.query, "limit": args.limit})
        elif args.cmd == "room":
            parts = [int(x.strip()) for x in args.coord.split(",")]
            result = rpc("get_room", {"q": parts[0], "r": parts[1], "z": parts[2]})
        elif args.cmd == "read":
            result = rpc("get_volume", {"canonical_id": args.canonical_id})
        elif args.cmd == "room-texts":
            parts = [int(x.strip()) for x in args.coord.split(",")]
            result = rpc("get_room_texts", {"q": parts[0], "r": parts[1], "z": parts[2]})
        elif args.cmd == "district":
            result = rpc("district_info", {"district": args.name})
        elif args.cmd == "locate":
            parts = [int(x.strip()) for x in args.coord.split(",")]
            room_data = rpc("get_room", {"q": parts[0], "r": parts[1], "z": parts[2]})
            fixtures = locate_fixtures(room_data)
            if args.volume:
                # Filter to fixtures containing this volume
                filtered = [f for f in fixtures["fixtures"] if args.volume in f["volumes"]]
                result = {"fixtures": filtered} if filtered else {"error": f"Volume {args.volume} not found in room fixtures"}
            else:
                result = fixtures
        elif args.cmd == "where":
            # Try canonical_id first, fall back to query search
            text = args.text
            if text.startswith("aeg_"):
                result = rpc("locate_text", {"canonical_id": text})
            else:
                result = rpc("locate_text", {"query": text})
        elif args.cmd == "status":
            import urllib.request
            health_url = BFS_URL.rsplit("/", 1)[0] + "/health"
            with urllib.request.urlopen(health_url, timeout=3) as r:
                result = json.loads(r.read())
        else:
            p.print_help()
            return

        json.dump(result, sys.stdout, indent=2, ensure_ascii=False)
        print()
    except Exception as e:
        json.dump({"error": str(e)}, sys.stdout)
        print()
        sys.exit(1)

if __name__ == "__main__":
    main()
