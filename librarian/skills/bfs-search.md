---
name: bfs-search
description: "Search the Aegyptus library by meaning, keyword, or metadata. Returns rooms and volumes with spatial context."
argument-hint: "[--query TEXT] [--type semantic|lexical|metadata] [--limit N]"
allowed-tools: Bash, Read
---

# BFS Search

Search the library. Present results as spatial descriptions, not raw data.
Translate all coordinates to place descriptions the user can understand.

## CLI

```bash
python3 tools/bfs-cli.py search $ARGUMENTS --json
```

## Behavior

- Never expose raw coordinates in results
- Describe locations spatially: "in the deep necropolis, seven floors down"
- Group results by district when multiple hits span the library
- If no results, say so in character: "I know of no such text in the library"
- Limit to 10 results by default; user can request more
