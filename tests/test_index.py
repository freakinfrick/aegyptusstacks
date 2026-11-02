"""Tests for L5 Index Service."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

import pytest

from bfs.index.service import IndexService
from bfs.resolver.resolver import SpatialResolver
from bfs.topology.graph import TopologyGraph
from bfs.topology.models import DoorEdge, WallEdge
from bfs.udm.models import SearchHit, SearchResult, VolumeContent


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture
def mock_adapter() -> MagicMock:
    adapter = MagicMock()
    adapter.search_lexical.return_value = [
        SearchResult(
            canonical_id="aeg_test_001",
            conventional_name="Test Papyrus",
            score=1.0,
            snippet="opening of the mouth",
        ),
        SearchResult(
            canonical_id="aeg_test_002",
            conventional_name="Test Stele",
            score=0.8,
            snippet="the mouth ritual",
        ),
    ]
    adapter.get_volume_content.return_value = VolumeContent(
        canonical_id="aeg_test_001",
        conventional_name="Test Papyrus",
        sentences=[],
        sentence_count=0,
        has_transliteration=True,
        has_translation=True,
        translation_languages=["en"],
    )
    return adapter


@pytest.fixture
def mock_resolver() -> MagicMock:
    resolver = MagicMock(spec=SpatialResolver)
    resolver._resolved = {
        "aeg_test_001": (5, -2, -3),
        "aeg_test_002": (5, -1, -3),
    }
    resolver.resolve.side_effect = lambda cid: resolver._resolved[cid]
    resolver.get_room_key.side_effect = SpatialResolver.get_room_key
    resolver._room_metadata = {
        "5:-2:-3": {
            "room_type": "chamber",
            "district": "house_of_life",
            "volume_count": 12,
            "z_band": -3,
        },
        "5:-1:-3": {
            "room_type": "hallway",
            "district": "house_of_life",
            "volume_count": 8,
            "z_band": -3,
        },
    }
    resolver._room_volumes = {
        "5:-2:-3": ["aeg_test_001"],
        "5:-1:-3": ["aeg_test_002"],
    }
    resolver._classifications = {}
    return resolver


@pytest.fixture
def mock_graph() -> MagicMock:
    graph = MagicMock(spec=TopologyGraph)
    graph.pathfind.return_value = [(5, -2, -3), (5, -1, -3)]
    graph.doors = {
        (5, -2, -3): [
            DoorEdge(from_coord=(5, -2, -3), to_coord=(5, -1, -3), direction="E"),
        ],
        (5, -1, -3): [
            DoorEdge(from_coord=(5, -1, -3), to_coord=(5, -2, -3), direction="W"),
        ],
    }
    graph.walls = {}
    graph.shafts = {}
    return graph


@pytest.fixture
def index_service(
    mock_adapter: MagicMock,
    mock_graph: MagicMock,
    mock_resolver: MagicMock,
) -> IndexService:
    return IndexService(
        adapter=mock_adapter,
        graph=mock_graph,
        resolver=mock_resolver,
    )


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

class TestIndexService:
    def test_import(self) -> None:
        """IndexService is importable."""
        from bfs.index.service import IndexService  # noqa: F811
        assert IndexService is not None

    def test_search_lexical_returns_search_hits(
        self, index_service: IndexService
    ) -> None:
        hits = index_service.search_lexical("mouth")
        assert isinstance(hits, list)
        assert len(hits) == 2
        assert all(isinstance(h, SearchHit) for h in hits)

    def test_search_lexical_annotates_coordinates(
        self, index_service: IndexService
    ) -> None:
        hits = index_service.search_lexical("mouth")
        assert hits[0].coord == (5, -2, -3)
        assert hits[0].district == "house_of_life"
        assert hits[0].z_band == -3

    def test_search_lexical_unresolved_text(
        self,
        mock_adapter: MagicMock,
        mock_graph: MagicMock,
        mock_resolver: MagicMock,
    ) -> None:
        """Texts not in resolved state get coord=None."""
        mock_adapter.search_lexical.return_value = [
            SearchResult(
                canonical_id="aeg_unknown",
                conventional_name="Unknown",
                score=0.5,
            ),
        ]
        svc = IndexService(adapter=mock_adapter, graph=mock_graph, resolver=mock_resolver)
        hits = svc.search_lexical("unknown")
        assert hits[0].coord is None
        assert hits[0].district is None

    def test_pathfind_returns_dict(self, index_service: IndexService) -> None:
        result = index_service.pathfind((5, -2, -3), (5, -1, -3))
        assert isinstance(result, dict)
        assert "path" in result
        assert "distance" in result
        assert result["distance"] == 2

    def test_neighborhood_returns_list(self, index_service: IndexService) -> None:
        rooms = index_service.neighborhood((5, -2, -3), radius=5)
        assert isinstance(rooms, list)
        assert len(rooms) >= 1

    def test_get_room_metadata(self, index_service: IndexService) -> None:
        meta = index_service.get_room_metadata(5, -2, -3)
        assert meta is not None
        assert meta["room_type"] == "chamber"
        assert meta["district"] == "house_of_life"

    def test_get_room_metadata_missing(self, index_service: IndexService) -> None:
        meta = index_service.get_room_metadata(999, 999, 999)
        assert meta is None

    def test_get_volume(self, index_service: IndexService) -> None:
        vol = index_service.get_volume("aeg_test_001")
        assert isinstance(vol, VolumeContent)
        assert vol.canonical_id == "aeg_test_001"

    def test_get_district_info(self, index_service: IndexService) -> None:
        info = index_service.get_district_info("house_of_life")
        assert isinstance(info, dict)
        assert info["district"] == "house_of_life"
        assert info["room_count"] == 2

    def test_filter_metadata_by_district(self, index_service: IndexService) -> None:
        hits = index_service.filter_metadata(district="house_of_life")
        assert isinstance(hits, list)
        # All hits should be in house_of_life district
        for h in hits:
            assert h.district == "house_of_life"
