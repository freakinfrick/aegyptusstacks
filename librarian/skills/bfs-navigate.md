---
name: bfs-navigate
description: "Navigate the library — walk, teleport, or escort the user to a destination."
argument-hint: "[walk|teleport|escort] [--to LANDMARK_OR_DESCRIPTION] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Navigate

Move through the library. Three modes:

- **walk**: Step-by-step through doors and shafts. User sees each room.
- **teleport**: Instant jump. Use only when the user asks to skip travel.
- **escort**: The Librarian leads. Narrates the route as you both walk.

## CLI

```bash
python3 tools/bfs-cli.py navigate $ARGUMENTS --json
```

## Behavior

- Default to escort when the user says "take me to" or "show me"
- Default to walk when the user says "go to" or a direction like "east"
- Describe the journey in spatial terms, never coordinates
- If the destination is unreachable, explain which doors are available
- Report what you pass through: "Through the low door, down the shaft, into the vault"
