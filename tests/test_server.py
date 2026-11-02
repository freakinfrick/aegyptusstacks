"""Tests for BFS server — import checks and RPC dispatch logic."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from bfs.server import (
    JsonRpcRequest,
    JsonRpcResponse,
    METHOD_MAP,
    handle_neighbors,
    handle_peek_wall,
)
from bfs.topology.models import DoorEdge, WallEdge


class TestImports:
    def test_app_importable(self) -> None:
        from bfs.server import app
        assert app is not None
        assert app.title == "BFS Service"

    def test_jsonrpc_models(self) -> None:
        req = JsonRpcRequest(method="get_room", params={"q": 0, "r": 0, "z": 0}, id=1)
        assert req.jsonrpc == "2.0"
        assert req.method == "get_room"

        resp = JsonRpcResponse(result={"test": True}, id=1)
        assert resp.jsonrpc == "2.0"
        assert resp.result == {"test": True}
        assert resp.error is None

    def test_error_response(self) -> None:
        resp = JsonRpcResponse(
            error={"code": -32601, "message": "Method not found"},
            id=1,
        )
        assert resp.error is not None
        assert resp.error["code"] == -32601


class TestMethodMap:
    def test_all_methods_registered(self) -> None:
        expected = {
            "get_room", "get_volume", "search", "pathfind",
            "neighbors", "peek_wall", "district_info",
        }
        assert set(METHOD_MAP.keys()) == expected

    def test_all_handlers_callable(self) -> None:
        for method, handler in METHOD_MAP.items():
            assert callable(handler), f"{method} handler is not callable"


class TestHandlers:
    def test_handle_neighbors_empty(self) -> None:
        """neighbors handler works with empty graph."""
        import bfs.server as srv
        original_graph = srv.graph
        try:
            mock_graph = MagicMock()
            mock_graph.doors = {}
            srv.graph = mock_graph
            result = handle_neighbors({"q": 0, "r": 0, "z": 0})
            assert result == []
        finally:
            srv.graph = original_graph

    def test_handle_neighbors_with_doors(self) -> None:
        import bfs.server as srv
        original_graph = srv.graph
        try:
            mock_graph = MagicMock()
            mock_graph.doors = {
                (1, 2, 3): [
                    DoorEdge(
                        from_coord=(1, 2, 3),
                        to_coord=(2, 2, 3),
                        direction="E",
                    )
                ],
            }
            srv.graph = mock_graph
            result = handle_neighbors({"q": 1, "r": 2, "z": 3})
            assert len(result) == 1
            assert result[0]["direction"] == "E"
        finally:
            srv.graph = original_graph

    def test_handle_peek_wall_no_wall(self) -> None:
        import bfs.server as srv
        original_graph = srv.graph
        original_resolver = srv.resolver
        try:
            mock_graph = MagicMock()
            mock_graph.walls = {}
            srv.graph = mock_graph
            srv.resolver = MagicMock()
            result = handle_peek_wall({"q": 0, "r": 0, "z": 0, "direction": "E"})
            assert result["to_coord"] is None
        finally:
            srv.graph = original_graph
            srv.resolver = original_resolver


class TestRpcEndpoint:
    """Test the /rpc endpoint via TestClient."""

    @pytest.fixture
    def client(self):
        from fastapi.testclient import TestClient
        from bfs.server import app
        return TestClient(app)

    def test_unknown_method(self, client) -> None:  # type: ignore[no-untyped-def]
        resp = client.post("/rpc", json={
            "jsonrpc": "2.0",
            "method": "nonexistent",
            "params": {},
            "id": 1,
        })
        assert resp.status_code == 200
        body = resp.json()
        assert body["error"] is not None
        assert body["error"]["code"] == -32601

    def test_health_endpoint(self, client) -> None:  # type: ignore[no-untyped-def]
        resp = client.get("/health")
        assert resp.status_code == 200
        assert resp.json()["status"] == "ok"
