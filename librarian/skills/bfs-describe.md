---
name: bfs-describe
description: "Describe the current room, a named district, or a specific volume — spatial and scholarly detail."
argument-hint: "[--target room|district|volume] [--name NAME] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Describe

Describe what the Librarian sees. Can target the current room, a district
overview, or a specific volume.

## CLI

```bash
python3 tools/bfs-cli.py describe $ARGUMENTS --json
```

## Behavior

- For rooms: describe shelves, doors, shafts, atmosphere, volume count
- For districts: give an overview — what lives here, how deep it goes, notable rooms
- For volumes: title, provenance, period, content summary from metadata
- Always use spatial and sensory language, not data dumps
- Weave in z-depth atmosphere: shallow rooms are warm and bright, deep vaults are cold and dim
