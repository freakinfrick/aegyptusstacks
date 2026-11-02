"""Tests for L7 Runtime State Manager."""

import pytest

from bfs.state.manager import ENTRANCE_COORD, StateManager, WearTracker


@pytest.fixture()
def manager() -> StateManager:
    return StateManager("redis://localhost:6379")


# ------------------------------------------------------------------
# Session lifecycle
# ------------------------------------------------------------------


def test_create_session(manager: StateManager) -> None:
    session = manager.create_session()
    assert session.current_position == ENTRANCE_COORD
    assert session.session_id
    assert len(session.traversal_history) == 1
    assert session.traversal_history[0].entered_via == "teleport"


def test_get_session(manager: StateManager) -> None:
    session = manager.create_session()
    loaded = manager.get_session(session.session_id)
    assert loaded is not None
    assert loaded.session_id == session.session_id
    assert loaded.current_position == ENTRANCE_COORD


def test_get_session_not_found(manager: StateManager) -> None:
    assert manager.get_session("nonexistent") is None


# ------------------------------------------------------------------
# Position updates
# ------------------------------------------------------------------


def test_update_position(manager: StateManager) -> None:
    session = manager.create_session()
    updated = manager.update_position(session.session_id, (1, 0, 0), "walk")
    assert updated.current_position == (1, 0, 0)
    assert len(updated.traversal_history) == 2
    # Previous entry should be closed
    assert updated.traversal_history[0].exited_at is not None
    assert updated.traversal_history[0].dwell_time_seconds is not None


def test_update_position_clears_volume(manager: StateManager) -> None:
    session = manager.create_session()
    # Simulate having a volume open by directly setting and persisting
    session.current_volume = "aeg_test"
    manager._persist(session)
    updated = manager.update_position(session.session_id, (2, 0, 0))
    assert updated.current_volume is None


def test_rooms_visited_no_duplicates(manager: StateManager) -> None:
    session = manager.create_session()
    manager.update_position(session.session_id, (1, 0, 0))
    updated = manager.update_position(session.session_id, ENTRANCE_COORD)
    # ENTRANCE_COORD was already in rooms_visited from creation
    assert updated.rooms_visited.count(ENTRANCE_COORD) == 1


def test_update_position_bad_session(manager: StateManager) -> None:
    with pytest.raises(KeyError):
        manager.update_position("bad_id", (0, 0, 0))


# ------------------------------------------------------------------
# Workspace
# ------------------------------------------------------------------


def test_workspace_add_and_get(manager: StateManager) -> None:
    session = manager.create_session()
    manager.add_to_workspace(session.session_id, "aeg_test_001", (5, 5, 0))
    workspace = manager.get_workspace(session.session_id)
    assert len(workspace) == 1
    assert workspace[0].canonical_id == "aeg_test_001"
    assert workspace[0].source_coord == (5, 5, 0)


def test_workspace_multiple(manager: StateManager) -> None:
    session = manager.create_session()
    manager.add_to_workspace(session.session_id, "aeg_001", (1, 0, 0))
    manager.add_to_workspace(session.session_id, "aeg_002", (2, 0, 0))
    workspace = manager.get_workspace(session.session_id)
    assert len(workspace) == 2


# ------------------------------------------------------------------
# Bookmarks
# ------------------------------------------------------------------


def test_add_bookmark(manager: StateManager) -> None:
    session = manager.create_session()
    updated = manager.add_bookmark(session.session_id, "pyramid_texts", (3, -1, 1))
    assert "pyramid_texts" in updated.bookmarks
    assert updated.bookmarks["pyramid_texts"] == (3, -1, 1)


# ------------------------------------------------------------------
# Wear tracking
# ------------------------------------------------------------------


def test_wear_zero_for_unvisited() -> None:
    tracker = WearTracker("redis://localhost:6379")
    # Use an unlikely coordinate to avoid test interference
    wear = tracker.get_wear((9999, 9999, 9999))
    assert wear == 0.0


def test_wear_increases_with_visits() -> None:
    tracker = WearTracker("redis://localhost:6379")
    coord = (9998, 9998, 9998)
    tracker.record_visit(coord)
    wear1 = tracker.get_wear(coord)
    tracker.record_visit(coord)
    wear2 = tracker.get_wear(coord)
    assert wear1 > 0.0
    assert wear2 >= wear1
