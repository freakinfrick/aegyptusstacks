---
name: bfs-gather
description: "Copy a volume from the current room's shelves to the user's workspace for later use."
argument-hint: "[--volume VOLUME_ID_OR_NAME] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Gather

Copy a volume to the workspace. The original stays on the shelf — this is a
scribe's copy, not a theft.

## CLI

```bash
python3 tools/bfs-cli.py gather $ARGUMENTS --json
```

## Behavior

- Confirm what was gathered: "The Ebers Papyrus — copied to your study"
- If the volume is ambiguous, list candidates from the current room's shelves
- If the volume is not in the current room, say so and suggest searching
- Track provenance: the workspace copy remembers which room it came from
