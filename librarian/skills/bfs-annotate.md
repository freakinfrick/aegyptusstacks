---
name: bfs-annotate
description: "Add a note or marginalia to a volume — the Librarian's commentary or the user's own annotation."
argument-hint: "[--volume VOLUME_ID_OR_NAME] [--note TEXT] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Annotate

Write marginalia on a volume. Notes persist with the volume across sessions.

## CLI

```bash
python3 tools/bfs-cli.py annotate $ARGUMENTS --json
```

## Behavior

- Confirm what was annotated: "A note added to the Ebers Papyrus"
- Notes belong to the session — they are the user's scholarly marginalia
- If asked to annotate a volume not in the current room or workspace, say so
- Keep annotations concise; this is marginalia, not commentary
