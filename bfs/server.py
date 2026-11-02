"""BFS Service — FastAPI app serving JSON-RPC 2.0 over Unix socket.

Entry point for the BFS spatial knowledge management service.
Initializes the full pipeline (adapter → resolver → topology → index)
on startup and serves queries via a single /rpc endpoint.
"""

from __future__ import annotations

import logging
import os
import sys
import threading
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

import uvicorn
from fastapi import FastAPI
from pydantic import BaseModel

from bfs.adapters.aegyptus import AegyptusAdapter
from bfs.index.service import IndexService
from bfs.resolver.resolver import SpatialResolver
from bfs.topology.graph import TopologyGraph

logger = logging.getLogger("bfs.server")


# ---------------------------------------------------------------------------
# JSON-RPC 2.0 models
# ---------------------------------------------------------------------------

class JsonRpcRequest(BaseModel):
    jsonrpc: str = "2.0"
    method: str
    params: dict[str, Any] = {}
    id: int | str | None = None


class JsonRpcResponse(BaseModel):
    jsonrpc: str = "2.0"
    result: Any = None
    error: dict[str, Any] | None = None
    id: int | str | None = None


# ---------------------------------------------------------------------------
# Service container (replaces bare module-level mutable globals)
# ---------------------------------------------------------------------------

class ServiceContainer:
    """Holds initialized service references with thread-safe access."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self.adapter: AegyptusAdapter | None = None
        self.resolver: SpatialResolver | None = None
        self.graph: TopologyGraph | None = None
        self.index: IndexService | None = None

    def require_index(self) -> IndexService:
        if self.index is None:
            raise RuntimeError("IndexService not initialized — server still starting up")
        return self.index

    def require_graph(self) -> TopologyGraph:
        if self.graph is None:
            raise RuntimeError("TopologyGraph not initialized — server still starting up")
        return self.graph

    def require_resolver(self) -> SpatialResolver:
        if self.resolver is None:
            raise RuntimeError("SpatialResolver not initialized — server still starting up")
        return self.resolver


services = ServiceContainer()


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DB_URL = os.environ.get("DATABASE_URL", "postgresql://aegyptus:dev@localhost:5434/aegyptus")
DATA_DIR = os.environ.get("BFS_DATA_DIR", "data/phase1")
STATE_DIR = os.environ.get("BFS_STATE_DIR", "bfs/data")


# ---------------------------------------------------------------------------
# Lifespan (replaces deprecated @app.on_event('startup'))
# ---------------------------------------------------------------------------

@asynccontextmanager
async def lifespan(app: FastAPI):
    state_path = Path(STATE_DIR) / "resolved_topology.json"

    # 1. Create adapter
    services.adapter = AegyptusAdapter(
        db_url=DB_URL,
        classifications_path=f"{DATA_DIR}/text_classifications.json",
        clusters_path=f"{DATA_DIR}/clusters.json",
    )
    logger.info("Adapter initialized")

    # 2. Load or build resolver state
    services.resolver = SpatialResolver(data_dir=DATA_DIR)
    if state_path.exists():
        services.resolver.load_state(state_path)
        logger.info("Resolver state loaded from %s", state_path)
    else:
        logger.info("No saved state found, running full resolution...")
        services.resolver.resolve_all()
        state_path.parent.mkdir(parents=True, exist_ok=True)
        services.resolver.save_state(state_path)
        logger.info("Resolution complete, state saved to %s", state_path)

    # 3. Build topology graph (may create corridor rooms in _state.room_metadata)
    services.graph = TopologyGraph()
    if services.resolver._state is not None:
        services.graph.build_from_resolver_state(services.resolver._state)
        # Sync: graph may have added corridor rooms to _state.room_metadata
        # that weren't in the resolver's frozen copy. Merge them in.
        for key, meta in services.resolver._state.room_metadata.items():
            if key not in services.resolver._room_metadata:
                services.resolver._room_metadata[key] = meta
    logger.info("Topology graph built: %s", services.graph.stats())

    # 4. Create index service
    services.index = IndexService(adapter=services.adapter, graph=services.graph, resolver=services.resolver)
    logger.info("Index service ready")

    yield


app = FastAPI(title="BFS Service", version="0.1.0", lifespan=lifespan)


# ---------------------------------------------------------------------------
# RPC handlers
# ---------------------------------------------------------------------------

def handle_get_room(params: dict[str, Any]) -> Any:
    """Get room metadata at (q, r, z)."""
    idx = services.require_index()
    q, r, z = params["q"], params["r"], params["z"]
    result = idx.get_room_metadata(q, r, z)
    if result is None:
        raise KeyError(f"No room at ({q}, {r}, {z})")
    return result


def handle_get_volume(params: dict[str, Any]) -> Any:
    """Get volume content by canonical_id."""
    idx = services.require_index()
    canonical_id = params["canonical_id"]
    volume = idx.get_volume(canonical_id)
    return volume.model_dump()


def handle_search(params: dict[str, Any]) -> Any:
    """Search for texts. type: 'lexical' (default)."""
    idx = services.require_index()
    query = params["query"]
    search_type = params.get("type", "lexical")
    limit = min(params.get("limit", 20), 500)

    if search_type == "lexical":
        fields = params.get("fields")
        hits = idx.search_lexical(query, fields, limit)
    elif search_type == "metadata":
        hits = idx.filter_metadata(
            district=params.get("district"),
            z_band=params.get("z_band"),
            script_type=params.get("script_type"),
            limit=limit,
        )
    else:
        raise ValueError(f"Unknown search type: {search_type}")

    return [h.model_dump() for h in hits]


def handle_pathfind(params: dict[str, Any]) -> Any:
    """Pathfind between two coordinates."""
    idx = services.require_index()
    start = (params["from_q"], params["from_r"], params["from_z"])
    end = (params["to_q"], params["to_r"], params["to_z"])
    return idx.pathfind(start, end)


def handle_neighbors(params: dict[str, Any]) -> Any:
    """Get door-connected neighbors of a room."""
    g = services.require_graph()
    coord = (params["q"], params["r"], params["z"])
    doors = g.doors.get(coord, [])
    return [
        {
            "from_coord": list(d.from_coord),
            "to_coord": list(d.to_coord),
            "direction": d.direction,
        }
        for d in doors
    ]


def handle_peek_wall(params: dict[str, Any]) -> Any:
    """Peek through a wall to see what's on the other side."""
    g = services.require_graph()
    res = services.require_resolver()
    coord = (params["q"], params["r"], params["z"])
    direction = params["direction"]

    walls = g.walls.get(coord, [])
    for w in walls:
        dq = w.to_coord[0] - w.from_coord[0]
        dr = w.to_coord[1] - w.from_coord[1]
        from bfs.topology.hex_utils import direction_name
        if direction_name(dq, dr) == direction:
            room_key = res.get_room_key(*w.to_coord)
            meta = res._room_metadata.get(room_key, {})
            return {
                "direction": direction,
                "to_coord": list(w.to_coord),
                "proximity_score": w.proximity_score,
                "room_type": meta.get("room_type", "unknown"),
                "volume_count": meta.get("volume_count", 0),
                "district": meta.get("district", "unknown"),
            }

    return {
        "direction": direction,
        "to_coord": None,
        "proximity_score": 0.0,
        "room_type": None,
        "volume_count": 0,
        "district": None,
    }


def handle_get_room_texts(params: dict[str, Any]) -> Any:
    """Batch-fetch all text content for a room — compact, LLM-friendly."""
    idx = services.require_index()
    q, r, z = params["q"], params["r"], params["z"]
    return idx.get_room_texts(q, r, z)


def handle_district_info(params: dict[str, Any]) -> Any:
    """Get district statistics."""
    idx = services.require_index()
    district = params["district"]
    return idx.get_district_info(district)


# ---------------------------------------------------------------------------
# RPC dispatch
# ---------------------------------------------------------------------------

METHOD_MAP: dict[str, Any] = {
    "get_room": handle_get_room,
    "get_volume": handle_get_volume,
    "get_room_texts": handle_get_room_texts,
    "search": handle_search,
    "pathfind": handle_pathfind,
    "neighbors": handle_neighbors,
    "peek_wall": handle_peek_wall,
    "district_info": handle_district_info,
}


@app.post("/rpc")
async def rpc(request: JsonRpcRequest) -> JsonRpcResponse:
    """Handle JSON-RPC 2.0 requests."""
    handler = METHOD_MAP.get(request.method)
    if not handler:
        return JsonRpcResponse(
            id=request.id,
            error={"code": -32601, "message": f"Method not found: {request.method}"},
        )
    try:
        result = handler(request.params)
        return JsonRpcResponse(id=request.id, result=result)
    except Exception as e:
        logger.exception("RPC error in %s", request.method)
        return JsonRpcResponse(
            id=request.id,
            error={"code": -32000, "message": str(e)},
        )


# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------

@app.get("/health")
async def health() -> dict[str, str]:
    return {"status": "ok", "service": "bfs"}


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(levelname)s %(message)s")
    socket_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/bfs.sock"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else None

    if port:
        # Binds 0.0.0.0 intentionally — local dev server, not exposed beyond localhost/tailscale
        uvicorn.run(app, host="0.0.0.0", port=port)
    else:
        uvicorn.run(app, uds=socket_path)
