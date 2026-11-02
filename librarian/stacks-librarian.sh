#!/bin/bash
# stacks-librarian — Unified corpus query CLI for the Librarian agent.
# Thin wrapper over bfs-query.py and Aegyptus CLI. All output is JSON.
set -euo pipefail

BFS="python3 $(dirname "$0")/bfs-query.py"
AEG="python3 $HOME/Aegyptus/cli.py"

cmd="${1:-help}"
shift || true

case "$cmd" in
  room|search|read|room-texts|district|locate|where|status)
    $BFS "$cmd" "$@"
    ;;
  grep)
    # Deep corpus search — searches translations, transliterations, names
    # Usage: grep PATTERN [--field translation|transliteration|name|token] [--limit N]
    $AEG grep "$@"
    ;;
  find)
    # Name/metadata search — finds texts by name, period, script type
    # Usage: find -q "sinuhe" [--limit N]
    $AEG search "$@"
    ;;
  translate)
    $AEG show --text "${1:?usage: translate CANONICAL_ID}" --pretty 2>/dev/null
    ;;
  help|*)
    cat <<'EOF'
stacks-librarian — Corpus query CLI

  where TEXT                      Locate a text's room — returns coord, district, room name
  search QUERY [--limit N]       BFS search — returns texts with room coords
  grep PATTERN [--field F] [--limit N]  F: transliteration|translation|name|token
  find -q NAME [--limit N]       Find texts by name/metadata
  room Q,R,Z                     Describe a room (structure, doors, shelves)
  room-texts Q,R,Z               All text content in a room (compact, batch)
  read CANONICAL_ID              Read a single volume's full text
  locate Q,R,Z [--volume ID]    Where texts are physically placed
  district NAME                  District overview
  translate CANONICAL_ID         Full text with translations
  status                         BFS health
EOF
    ;;
esac
