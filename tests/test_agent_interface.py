"""Tests for L6 Agent Interface — tier separation, coordinate validation, purity."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import pytest

from bfs.agent_interface.interface import AgentInterface
from bfs.agent_interface.tier1_retrieval import RetrievalTools
from bfs.agent_interface.tier2_spatial import SpatialTools
from bfs.agent_interface.tier3_narration import NarrationTools
from bfs.topology.graph import TopologyGraph


# ------------------------------------------------------------------
# Mock collaborators
# ------------------------------------------------------------------


class FakeSession:
    """Minimal session object for tests."""

    def __init__(self, position: tuple[int, int, int] = (0, 0, 3)) -> None:
        self.current_position = position
        self.current_volume: str | None = None
        self.volumes_opened: set[str] = set()
        self.workspace_annotations: list[dict[str, Any]] = []
        self.bookmarks: dict[str, tuple[int, int, int]] = {}
        self.shelf_highlights: list[dict[str, Any]] = []


class FakeStateManager:
    """In-memory state manager for testing."""

    def __init__(self) -> None:
        self.sessions: dict[str, FakeSession] = {}
        self.workspace: dict[str, list[dict[str, Any]]] = {}
        self.position_updates: list[tuple[str, tuple[int, int, int], str]] = []

    def create_session(
        self, session_id: str, start_position: tuple[int, int, int] = (0, 0, 3)
    ) -> FakeSession:
        session = FakeSession(start_position)
        self.sessions[session_id] = session
        return session

    def get_session(self, session_id: str) -> FakeSession:
        if session_id not in self.sessions:
            self.sessions[session_id] = FakeSession()
        return self.sessions[session_id]

    def update_position(
        self,
        session_id: str,
        new_pos: tuple[int, int, int],
        via: str = "walk",
    ) -> None:
        session = self.get_session(session_id)
        session.current_position = new_pos
        self.position_updates.append((session_id, new_pos, via))

    def add_to_workspace(
        self,
        session_id: str,
        canonical_id: str,
        source_coord: tuple[int, int, int],
    ) -> None:
        if session_id not in self.workspace:
            self.workspace[session_id] = []
        self.workspace[session_id].append(
            {"canonical_id": canonical_id, "source_coord": source_coord}
        )


@dataclass
class FakeSearchResult:
    canonical_id: str
    conventional_name: str | None = None
    score: float = 1.0
    snippet: str | None = None
    coord: tuple[int, int, int] | None = None

    def model_dump(self) -> dict[str, Any]:
        return {
            "canonical_id": self.canonical_id,
            "conventional_name": self.conventional_name,
            "score": self.score,
            "snippet": self.snippet,
            "coord": self.coord,
        }


class FakeIndexService:
    """Minimal index service for testing."""

    def __init__(self) -> None:
        self.search_calls: list[tuple[str, int]] = []

    def search_lexical(self, query: str, limit: int = 10) -> list[FakeSearchResult]:
        self.search_calls.append((query, limit))
        if "pyramid" in query.lower():
            return [
                FakeSearchResult(
                    canonical_id="pyr_001",
                    conventional_name="Pyramid Texts Unas",
                    score=1.0,
                    snippet="Opening of the mouth...",
                ),
            ]
        return []


@dataclass
class FakeVolumeContent:
    canonical_id: str
    conventional_name: str | None = None
    sentences: list[Any] = field(default_factory=list)
    sentence_count: int = 0
    has_transliteration: bool = False
    has_translation: bool = False
    translation_languages: list[str] = field(default_factory=list)

    def model_dump(self) -> dict[str, Any]:
        return {
            "canonical_id": self.canonical_id,
            "conventional_name": self.conventional_name,
            "sentences": self.sentences,
            "sentence_count": self.sentence_count,
            "has_transliteration": self.has_transliteration,
            "has_translation": self.has_translation,
            "translation_languages": self.translation_languages,
        }


class FakeAdapter:
    """Minimal adapter for testing."""

    def get_volume_content(self, canonical_id: str) -> FakeVolumeContent:
        return FakeVolumeContent(
            canonical_id=canonical_id,
            conventional_name=f"Text {canonical_id}",
            sentence_count=5,
        )


@dataclass
class FakeResolverState:
    room_assignments: dict[str, tuple[int, int, int]]
    room_volumes: dict[str, list[str]]
    room_metadata: dict[str, dict[str, Any]]
    cluster_rooms: dict[str, list[str]]
    district_stats: dict[str, int]
    z_band_stats: dict[str, int]
    total_rooms: int = 0
    total_texts: int = 0
    seed: int = 42


class FakeResolver:
    """Minimal resolver for testing."""

    def __init__(self) -> None:
        self._resolved: dict[str, tuple[int, int, int]] = {
            "text_001": (0, 0, 0),
            "text_002": (1, 0, 0),
            "text_003": (1, -1, 0),
            "text_004": (2, 0, -1),
            "pyr_001": (0, 0, 0),
        }
        self._occupied: set[tuple[int, int, int]] = {
            (0, 0, 0), (1, 0, 0), (1, -1, 0), (2, 0, -1), (0, 0, 3),
        }
        self._landmarks: dict[str, int] = {"pyr_001": -3}
        self._state = FakeResolverState(
            room_assignments=dict(self._resolved),
            room_volumes={
                "0:0:0": ["text_001", "pyr_001"],
                "1:0:0": ["text_002"],
                "1:-1:0": ["text_003"],
                "2:0:-1": ["text_004"],
                "0:0:3": [],
            },
            room_metadata={
                "0:0:0": {
                    "room_type": "chamber",
                    "district": "temple",
                    "volume_count": 2,
                    "shelves": [["text_001", "pyr_001"]],
                    "palette": "warm_amber",
                },
                "1:0:0": {
                    "room_type": "hallway",
                    "district": "temple",
                    "volume_count": 1,
                    "shelves": [["text_002"]],
                    "palette": "warm_amber",
                },
                "1:-1:0": {
                    "room_type": "chamber",
                    "district": "house_of_life",
                    "volume_count": 1,
                    "shelves": [["text_003"]],
                    "palette": "warm_amber",
                },
                "2:0:-1": {
                    "room_type": "vault",
                    "district": "necropolis",
                    "volume_count": 1,
                    "shelves": [["text_004"]],
                    "palette": "warm_amber",
                },
                "0:0:3": {
                    "room_type": "rotunda",
                    "district": "observatory",
                    "volume_count": 0,
                    "shelves": [],
                    "palette": "warm_amber",
                },
            },
            cluster_rooms={"cluster_a": ["0:0:0", "1:0:0"]},
            district_stats={"temple": 3, "house_of_life": 1, "necropolis": 1},
            z_band_stats={"0": 3, "-1": 1},
        )

    @staticmethod
    def get_room_key(q: int, r: int, z: int) -> str:
        return f"{q}:{r}:{z}"


# ------------------------------------------------------------------
# Fixtures
# ------------------------------------------------------------------


@pytest.fixture
def graph() -> TopologyGraph:
    """Build a small test topology graph."""
    g = TopologyGraph()
    # Create a small connected graph:
    # (0,0,0) -- E --> (1,0,0) -- NE --> (1,-1,0)
    # (0,0,3) shaft to (0,0,0)
    # (2,0,-1) connected to (1,0,0)
    g.add_door((0, 0, 0), (1, 0, 0))
    g.add_door((1, 0, 0), (1, -1, 0))
    g.add_door((1, 0, 0), (2, 0, -1))
    g.add_shaft((0, 0, 3), (0, 0, 0))
    return g


@pytest.fixture
def resolver() -> FakeResolver:
    return FakeResolver()


@pytest.fixture
def index_service() -> FakeIndexService:
    return FakeIndexService()


@pytest.fixture
def state_manager() -> FakeStateManager:
    return FakeStateManager()


@pytest.fixture
def adapter() -> FakeAdapter:
    return FakeAdapter()


@pytest.fixture
def retrieval(
    index_service: FakeIndexService,
    resolver: FakeResolver,
    graph: TopologyGraph,
) -> RetrievalTools:
    return RetrievalTools(index_service, resolver, graph)


@pytest.fixture
def spatial(
    state_manager: FakeStateManager,
    resolver: FakeResolver,
    graph: TopologyGraph,
    adapter: FakeAdapter,
) -> SpatialTools:
    return SpatialTools(state_manager, resolver, graph, adapter)


@pytest.fixture
def narration(state_manager: FakeStateManager) -> NarrationTools:
    return NarrationTools(state_manager)


@pytest.fixture
def interface(
    retrieval: RetrievalTools,
    spatial: SpatialTools,
    narration: NarrationTools,
) -> AgentInterface:
    return AgentInterface(retrieval, spatial, narration)


# ------------------------------------------------------------------
# Tier 1 — Retrieval (Pure Reads)
# ------------------------------------------------------------------


class TestTier1Retrieval:
    """Tier 1 operations: no side effects, returns data."""

    def test_lexical_search_delegates_to_index(
        self, retrieval: RetrievalTools, index_service: FakeIndexService
    ) -> None:
        results = retrieval.lexical_search("pyramid")
        assert len(results) == 1
        assert results[0]["canonical_id"] == "pyr_001"
        assert index_service.search_calls == [("pyramid", 10)]

    def test_lexical_search_enriches_with_coords(
        self, retrieval: RetrievalTools
    ) -> None:
        results = retrieval.lexical_search("pyramid")
        assert results[0]["coord"] == (0, 0, 0)

    def test_semantic_search_delegates_to_lexical(
        self, retrieval: RetrievalTools, index_service: FakeIndexService
    ) -> None:
        retrieval.semantic_search("pyramid", limit=5)
        assert index_service.search_calls == [("pyramid", 5)]

    def test_empty_search(self, retrieval: RetrievalTools) -> None:
        results = retrieval.lexical_search("nonexistent")
        assert results == []

    def test_neighborhood_returns_nearby_rooms(
        self, retrieval: RetrievalTools
    ) -> None:
        results = retrieval.neighborhood(0, 0, 0, radius=2)
        coords = [r["coord"] for r in results]
        assert (0, 0, 0) in coords
        assert (1, 0, 0) in coords  # distance 1
        assert (1, -1, 0) in coords  # distance 1

    def test_neighborhood_excludes_distant_rooms(
        self, retrieval: RetrievalTools
    ) -> None:
        results = retrieval.neighborhood(0, 0, 0, radius=0)
        assert len(results) == 1
        assert results[0]["coord"] == (0, 0, 0)

    def test_pathfind_finds_route(self, retrieval: RetrievalTools) -> None:
        result = retrieval.pathfind(0, 0, 0, 1, -1, 0)
        assert result["found"] is True
        assert result["steps"] == 2
        assert result["path"][0] == (0, 0, 0)
        assert result["path"][-1] == (1, -1, 0)

    def test_pathfind_no_route(self, retrieval: RetrievalTools) -> None:
        # No room at (99, 99, 99)
        result = retrieval.pathfind(0, 0, 0, 99, 99, 99)
        assert result["found"] is False
        assert result["path"] == []

    def test_describe_volume(self, retrieval: RetrievalTools) -> None:
        result = retrieval.describe_volume("text_001")
        assert result["canonical_id"] == "text_001"
        assert result["coord"] == (0, 0, 0)
        assert result["district"] == "temple"

    def test_describe_district(self, retrieval: RetrievalTools) -> None:
        result = retrieval.describe_district("temple")
        assert result["name"] == "temple"
        assert result["text_count"] == 3
        assert "room_count" in result

    def test_describe_unknown_district(self, retrieval: RetrievalTools) -> None:
        result = retrieval.describe_district("atlantis")
        assert "error" in result

    def test_list_landmarks(self, retrieval: RetrievalTools) -> None:
        landmarks = retrieval.list_landmarks()
        labels = [lm["label"] for lm in landmarks]
        assert "The Observatory" in labels
        assert "pyr_001" in labels

    def test_metadata_filter_by_district(self, retrieval: RetrievalTools) -> None:
        results = retrieval.metadata_filter(district="temple")
        assert len(results) > 0
        for r in results:
            assert r["district"] == "temple"

    def test_tier1_does_not_mutate_state(
        self,
        retrieval: RetrievalTools,
        state_manager: FakeStateManager,
    ) -> None:
        """Core guarantee: Tier 1 operations produce no side effects."""
        retrieval.lexical_search("pyramid")
        retrieval.neighborhood(0, 0, 0, radius=2)
        retrieval.pathfind(0, 0, 0, 1, 0, 0)
        retrieval.describe_volume("text_001")
        retrieval.describe_district("temple")
        retrieval.list_landmarks()
        # State manager should have zero interactions
        assert state_manager.position_updates == []
        assert state_manager.workspace == {}
        assert state_manager.sessions == {}


# ------------------------------------------------------------------
# Tier 2 — Spatial (Session State Mutations)
# ------------------------------------------------------------------


class TestTier2Spatial:
    """Tier 2 operations: require valid coordinates, mutate session state."""

    def test_teleport_valid_coord(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 3))
        result = spatial.teleport("s1", 1, 0, 0)
        assert result["position"] == (1, 0, 0)
        assert state_manager.position_updates[-1] == ("s1", (1, 0, 0), "teleport")

    def test_teleport_invalid_coord_raises(self, spatial: SpatialTools) -> None:
        with pytest.raises(ValueError, match="Invalid coordinate"):
            spatial.teleport("s1", 99, 99, 99)

    def test_walk_to_produces_path(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.walk_to("s1", 1, -1, 0)
        assert result["steps"] == 2
        assert len(result["path"]) == 3
        # Should have updated position for each step
        assert len(state_manager.position_updates) == 2

    def test_walk_to_same_position(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.walk_to("s1", 0, 0, 0)
        assert result["steps"] == 0

    def test_validate_coord_caches(self, spatial: SpatialTools) -> None:
        assert spatial._valid_coords is None
        spatial._validate_coord(0, 0, 0)
        assert spatial._valid_coords is not None
        assert (0, 0, 0) in spatial._valid_coords

    def test_describe_room(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.describe_room("s1")
        assert result["coord"] == (0, 0, 0)
        assert result["room_type"] == "chamber"
        assert "doors" in result

    def test_fetch_to_workspace(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1")
        result = spatial.fetch_to_workspace("s1", "text_001")
        assert result["status"] == "gathered"
        assert result["source_coord"] == (0, 0, 0)
        assert len(state_manager.workspace["s1"]) == 1

    def test_fetch_unknown_volume_raises(self, spatial: SpatialTools) -> None:
        with pytest.raises(ValueError, match="Unknown volume"):
            spatial.fetch_to_workspace("s1", "nonexistent_volume")

    def test_examine_shelf(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.examine_shelf("s1", 0)
        assert result["shelf_index"] == 0
        assert "text_001" in result["volumes"]

    def test_examine_shelf_out_of_range(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.examine_shelf("s1", 99)
        assert "error" in result

    def test_open_volume(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1")
        result = spatial.open_volume("s1", "text_001")
        assert result["canonical_id"] == "text_001"

    def test_peek_through_door(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.peek_through_door("s1", "E")
        assert result["coord"] == (1, 0, 0)
        # peek should NOT update position
        session = state_manager.get_session("s1")
        assert session.current_position == (0, 0, 0)

    def test_peek_no_door(
        self, spatial: SpatialTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 0))
        result = spatial.peek_through_door("s1", "S")
        assert "error" in result


# ------------------------------------------------------------------
# Tier 3 — Narration (Overlay Mutations)
# ------------------------------------------------------------------


class TestTier3Narration:
    """Tier 3 operations: overlay mutations, never affect base topology."""

    def test_annotate(
        self, narration: NarrationTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1")
        result = narration.annotate("s1", "text_001", "Important passage")
        assert result["status"] == "annotated"
        assert result["note"] == "Important passage"

    def test_bookmark(
        self, narration: NarrationTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1")
        result = narration.bookmark("s1", 0, 0, 0, "My Study")
        assert result["status"] == "bookmarked"
        assert result["label"] == "My Study"
        session = state_manager.get_session("s1")
        assert session.bookmarks["My Study"] == (0, 0, 0)

    def test_highlight_shelf(
        self, narration: NarrationTools, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1")
        result = narration.highlight_shelf("s1", 0, 0, 0, 0, color="gold")
        assert result["status"] == "highlighted"
        assert result["color"] == "gold"

    def test_narration_does_not_affect_topology(
        self,
        narration: NarrationTools,
        state_manager: FakeStateManager,
        graph: TopologyGraph,
    ) -> None:
        """Core guarantee: Tier 3 never modifies base topology."""
        stats_before = graph.stats()
        state_manager.create_session("s1")
        narration.annotate("s1", "text_001", "note")
        narration.bookmark("s1", 0, 0, 0, "mark")
        narration.highlight_shelf("s1", 0, 0, 0, 0)
        stats_after = graph.stats()
        assert stats_before == stats_after


# ------------------------------------------------------------------
# AgentInterface — Unified Entry Point
# ------------------------------------------------------------------


class TestAgentInterface:
    """Test the unified interface with tier enforcement."""

    def test_execute_tier1(self, interface: AgentInterface) -> None:
        result = interface.execute(1, "describe_district", district="temple")
        assert result["name"] == "temple"

    def test_execute_tier2_valid_coord(
        self, interface: AgentInterface, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1", start_position=(0, 0, 3))
        result = interface.execute(2, "teleport", session_id="s1", q=1, r=0, z=0)
        assert result["position"] == (1, 0, 0)

    def test_execute_tier2_invalid_coord_rejected(
        self, interface: AgentInterface
    ) -> None:
        """Tier 2 calls with invalid coordinates are rejected at the interface."""
        with pytest.raises(ValueError, match="Invalid coordinate"):
            interface.execute(
                2, "teleport", session_id="s1", q=99, r=99, z=99
            )

    def test_execute_tier3(
        self, interface: AgentInterface, state_manager: FakeStateManager
    ) -> None:
        state_manager.create_session("s1")
        result = interface.execute(
            3, "annotate", session_id="s1", canonical_id="text_001", note="test"
        )
        assert result["status"] == "annotated"

    def test_execute_invalid_tier(self, interface: AgentInterface) -> None:
        with pytest.raises(ValueError, match="Invalid tier"):
            interface.execute(4, "anything")

    def test_execute_unknown_method(self, interface: AgentInterface) -> None:
        with pytest.raises(ValueError, match="Unknown method"):
            interface.execute(1, "nonexistent_method")

    def test_execute_private_method_rejected(self, interface: AgentInterface) -> None:
        with pytest.raises(ValueError, match="Unknown method"):
            interface.execute(2, "_validate_coord")

    def test_validate_coordinate(self, interface: AgentInterface) -> None:
        assert interface.validate_coordinate(0, 0, 0) is True
        assert interface.validate_coordinate(99, 99, 99) is False

    def test_list_methods(self, interface: AgentInterface) -> None:
        tier1_methods = interface.list_methods(1)
        assert "lexical_search" in tier1_methods
        assert "semantic_search" in tier1_methods
        assert "neighborhood" in tier1_methods

        tier2_methods = interface.list_methods(2)
        assert "teleport" in tier2_methods
        assert "walk_to" in tier2_methods

        tier3_methods = interface.list_methods(3)
        assert "annotate" in tier3_methods
        assert "bookmark" in tier3_methods

    def test_tier1_cannot_reach_tier2_methods(
        self, interface: AgentInterface
    ) -> None:
        """Tier separation: can't call tier 2 methods through tier 1."""
        with pytest.raises(ValueError, match="Unknown method"):
            interface.execute(1, "teleport", session_id="s1", q=0, r=0, z=0)

    def test_coord_validation_on_tier3(
        self, interface: AgentInterface, state_manager: FakeStateManager
    ) -> None:
        """Tier 3 also validates coordinates."""
        state_manager.create_session("s1")
        with pytest.raises(ValueError, match="Invalid coordinate"):
            interface.execute(
                3, "bookmark",
                session_id="s1", q=99, r=99, z=99, label="bad"
            )
