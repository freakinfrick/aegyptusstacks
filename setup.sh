#!/usr/bin/env bash
set -euo pipefail

# Aegyptus Labyrinth — Bootstrap Script
# Sets up the database, loads the corpus, starts BFS, and builds the game.

REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$REPO_ROOT"

echo "=== Aegyptus Labyrinth Setup ==="
echo ""

# ── Check prerequisites ──
missing=()
command -v docker >/dev/null 2>&1 || missing+=("docker")
command -v cargo >/dev/null 2>&1  || missing+=("cargo (rustup.rs)")
command -v python3 >/dev/null 2>&1 || missing+=("python3")
command -v curl >/dev/null 2>&1   || missing+=("curl")
command -v tmux >/dev/null 2>&1   || missing+=("tmux (optional, for Librarian NPC)")

if [ ${#missing[@]} -gt 0 ]; then
    echo "Missing prerequisites:"
    for m in "${missing[@]}"; do
        echo "  - $m"
    done
    echo ""
    echo "Install them and re-run this script."
    exit 1
fi

# ── Check Python version (3.9+ required) ──
PY_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
PY_MAJOR=$(echo "$PY_VERSION" | cut -d. -f1)
PY_MINOR=$(echo "$PY_VERSION" | cut -d. -f2)
if [ "$PY_MAJOR" -lt 3 ] || { [ "$PY_MAJOR" -eq 3 ] && [ "$PY_MINOR" -lt 9 ]; }; then
    echo "ERROR: Python 3.9+ required, found $PY_VERSION"
    exit 1
fi
echo "  Python: $PY_VERSION ✓"

# ── Check system libraries for Rust native deps ──
build_deps_missing=()
if ! pkg-config --exists openssl 2>/dev/null; then
    build_deps_missing+=("libssl-dev (or openssl-devel)")
fi
if ! command -v pkg-config >/dev/null 2>&1; then
    build_deps_missing+=("pkg-config")
fi
if ! command -v cc >/dev/null 2>&1; then
    build_deps_missing+=("build-essential (or gcc)")
fi
if [ ${#build_deps_missing[@]} -gt 0 ]; then
    echo ""
    echo "Missing build dependencies (needed for Rust compilation):"
    for m in "${build_deps_missing[@]}"; do
        echo "  - $m"
    done
    echo ""
    echo "On Debian/Ubuntu:  sudo apt install build-essential pkg-config libssl-dev"
    echo "On Fedora/RHEL:    sudo dnf install gcc pkg-config openssl-devel"
    echo "On Arch:           sudo pacman -S base-devel openssl"
    exit 1
fi

# ── Environment file ──
if [ ! -f .env ]; then
    cp .env.example .env
    echo "[1/6] Created .env from .env.example"
    echo "      Edit .env to set API keys if you want Scribe article generation."
else
    echo "[1/6] .env already exists"
fi

# ── Python virtual environment ──
VENV_DIR="$REPO_ROOT/.venv"
if [ ! -d "$VENV_DIR" ]; then
    echo "[2/6] Creating Python virtual environment..."
    python3 -m venv "$VENV_DIR"
fi
# shellcheck source=/dev/null
source "$VENV_DIR/bin/activate"

if ! python3 -c "import fastapi" 2>/dev/null; then
    echo "      Installing Python dependencies..."
    pip install -r requirements.txt -q
fi
echo "  venv: $VENV_DIR ✓"

# ── Start PostgreSQL ──
echo "[3/6] Starting PostgreSQL..."
docker compose up -d db
echo "      Waiting for database to be ready..."
for i in $(seq 1 30); do
    if docker compose exec -T db pg_isready -U aegyptus >/dev/null 2>&1; then
        echo "      Database ready."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "      ERROR: Database did not become ready in time."
        exit 1
    fi
    sleep 1
done

# ── Load corpus if empty ──
ROW_COUNT=$(docker compose exec -T db psql -U aegyptus -d aegyptus -t -c \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'texts';" 2>/dev/null | tr -d ' ' || echo "0")

if [ "$ROW_COUNT" = "0" ] || [ "$ROW_COUNT" = "" ]; then
    echo "[4/6] Loading corpus (this takes a few minutes on first run)..."
    if [ -f data/corpus/aegyptus.sql.gz ]; then
        gunzip -c data/corpus/aegyptus.sql.gz | docker compose exec -T db psql -U aegyptus -d aegyptus -q
        echo "      Corpus loaded: 77,683 texts, 460,333 sentences."
    else
        echo "      ERROR: data/corpus/aegyptus.sql.gz not found."
        echo "      Build it with https://github.com/freakinfrick/aegyptusdb (see README.md, Corpus database)."
        exit 1
    fi
else
    # Check if texts table has data
    TEXT_COUNT=$(docker compose exec -T db psql -U aegyptus -d aegyptus -t -c \
        "SELECT count(*) FROM texts;" 2>/dev/null | tr -d ' ' || echo "0")
    echo "[4/6] Corpus already loaded ($TEXT_COUNT texts)"
fi

# ── Start BFS server ──
echo "[5/6] Starting BFS server..."

# Source .env for DATABASE_URL
set -a
source .env 2>/dev/null || true
set +a

# Initialize topology if needed
if [ ! -f bfs/data/resolved_topology.json ]; then
    mkdir -p bfs/data
    if [ -f data/topology/resolved_topology.json ]; then
        echo "      Copying pre-built topology..."
        cp data/topology/resolved_topology.json bfs/data/
    else
        echo "      Building topology (first run, ~2 min)..."
        python3 -m bfs.resolver.initialize
    fi
fi

# Create articles directory
mkdir -p "$REPO_ROOT/articles"

# Start BFS server in background
pkill -f "python3 -m bfs.server" 2>/dev/null || true
sleep 1
BFS_PORT="${BFS_PORT:-8081}"
python3 -m bfs.server /tmp/bfs.sock "$BFS_PORT" &
BFS_PID=$!
echo "      BFS server starting (PID $BFS_PID, port $BFS_PORT)..."

# Write PID file for cleanup
echo "$BFS_PID" > "$REPO_ROOT/.bfs.pid"

# Wait for ready
for i in $(seq 1 30); do
    if curl -sf "http://localhost:$BFS_PORT/health" >/dev/null 2>&1; then
        echo "      BFS server ready."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "      WARNING: BFS server did not respond in time. Check logs."
    fi
    sleep 1
done

# ── Build Rust TUI ──
echo "[6/6] Building stacks-engine..."
cargo build -p stacks-engine --release 2>&1 | tail -3

echo ""
echo "=== Setup Complete ==="
echo ""
echo "  Play:     cargo run -p stacks-engine --release"
echo "  BFS CLI:  python3 librarian/bfs-query.py search 'heart'"
echo "  BFS PID:  $BFS_PID (port $BFS_PORT)"
echo "  Stop BFS: kill \$(cat .bfs.pid)"
echo ""
echo "  Optional:"
echo "    - Set SCRIBE_API_KEY in .env for article generation"
echo "    - Install hermes-agent for the Librarian NPC (see docs/HERMES_INTEGRATION_SPEC.md)"
echo ""
