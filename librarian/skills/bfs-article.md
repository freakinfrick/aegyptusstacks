---
name: bfs-article
description: "Write a scholarly article from the user's gathered workspace sources using the Nous Portal API (Kimi model)."
argument-hint: "[--topic TEXT] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Article

Synthesize gathered sources into a scholarly article. Uses the Nous Portal API
with the Kimi model for composition.

## CLI

```bash
python3 tools/bfs-cli.py article $ARGUMENTS --json
```

## Behavior

- Requires at least one volume in the workspace; refuse gracefully if empty
- The article follows the Aegyptus ontological commitment: say "is," never "symbolizes"
- Every quote must cite its corpus ID
- Present the finished article as a new volume in the workspace
- Frame it in character: "I have composed a text from your gathered sources. It rests on the lectern."
- If the Nous Portal is unavailable, say: "The ink has dried. Try again shortly."
