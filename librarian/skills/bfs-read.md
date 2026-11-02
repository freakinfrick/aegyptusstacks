---
name: bfs-read
description: "Open a volume for the user to read — displays transliteration and translation on the lectern."
argument-hint: "[--volume VOLUME_ID_OR_NAME] [--page N] [--session SESSION_ID]"
allowed-tools: Bash, Read
---

# BFS Read

Open a volume on the lectern. Shows transliteration and translation side by side.

## CLI

```bash
python3 tools/bfs-cli.py read $ARGUMENTS --json
```

## Behavior

- Present the text naturally: "This is Utterance 600 of the Pyramid Texts..."
- Show transliteration and translation together
- If the volume has many sentences, show the first page and offer to continue
- Never fabricate content — display only what the corpus contains
- If the user asks about a word or passage, answer from the text itself
