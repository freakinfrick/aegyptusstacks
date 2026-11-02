---
name: bfs-pathfind
description: "Find the walking route between two locations in the library — doors, shafts, and estimated distance."
argument-hint: "[--from LANDMARK_OR_HERE] [--to LANDMARK_OR_DESCRIPTION] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Pathfind

Find a route between two points in the library. Returns the path as a sequence
of rooms, doors, and shafts.

## CLI

```bash
python3 tools/bfs-cli.py pathfind $ARGUMENTS --json
```

## Behavior

- Default --from to the user's current position
- Describe the route spatially: "East through three chambers, then down the shaft to floor minus four"
- Report estimated distance in rooms, not coordinates
- If no path exists, explain why: "That wing is sealed" or "No shaft connects those floors here"
- Prefer routes through interesting rooms when multiple paths exist
