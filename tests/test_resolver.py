"""Tests for BFS L3 Spatial Resolver."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from bfs.resolver.constants import CANONICAL_SEED, MAX_VOLUMES_PER_ROOM, hex_distance
from bfs.resolver.resolver import SpatialResolver, _hex_ring, _hex_spiral_coords
from bfs.resolver.z_bands import compute_z_band

DATA_DIR = Path(__file__).resolve().parent.parent / "data" / "phase1"


# ------------------------------------------------------------------
# Fixtures
# ------------------------------------------------------------------

@pytest.fixture(scope="module")
def all_ids() -> list[str]:
    with open(DATA_DIR / "text_classifications.json") as f:
        return list(json.load(f).keys())


@pytest.fixture(scope="module")
def sample_ids(all_ids: list[str]) -> list[str]:
    """A deterministic sample of 500 IDs for fast tests."""
    import random
    rng = random.Random(42)
    return rng.sample(all_ids, min(500, len(all_ids)))


@pytest.fixture(scope="module")
def full_resolver(all_ids: list[str]) -> SpatialResolver:
    """Resolver with full corpus resolved (shared across module)."""
    r = SpatialResolver(CANONICAL_SEED, DATA_DIR)
    r.resolve_all(all_ids)
    return r


@pytest.fixture(scope="module")
def full_state(full_resolver: SpatialResolver) -> dict:
    """The ResolverState from the full resolution."""
    assert full_resolver._state is not None
    return full_resolver._state


# ------------------------------------------------------------------
# Determinism
# ------------------------------------------------------------------

class TestDeterminism:
    def test_same_seed_same_output(self, sample_ids: list[str]) -> None:
        r1 = SpatialResolver(CANONICAL_SEED, DATA_DIR)
        r1.resolve_all(sample_ids)
        r2 = SpatialResolver(CANONICAL_SEED, DATA_DIR)
        r2.resolve_all(sample_ids)
        for cid in sample_ids:
            assert r1.resolve(cid) == r2.resolve(cid), f"Non-deterministic for {cid}"

    def test_different_seed_different_output(self, sample_ids: list[str]) -> None:
        r1 = SpatialResolver(CANONICAL_SEED, DATA_DIR)
        r1.resolve_all(sample_ids)
        r2 = SpatialResolver(CANONICAL_SEED + 1, DATA_DIR)
        r2.resolve_all(sample_ids)
        # At least some texts should differ
        diffs = sum(1 for cid in sample_ids if r1.resolve(cid) != r2.resolve(cid))
        assert diffs > 0, "Different seeds produced identical outputs"


# ------------------------------------------------------------------
# Landmark texts
# ------------------------------------------------------------------

class TestLandmarks:
    def test_pyramid_texts_deep(self, full_resolver: SpatialResolver) -> None:
        pt = full_resolver.resolve("aeg_oraec_502")
        assert pt[2] == -7, f"Pyramid Texts at z={pt[2]}, expected -7"

    def test_cannibal_hymn_deep(self, full_resolver: SpatialResolver) -> None:
        ch = full_resolver.resolve("aeg_oraec_226")
        assert ch[2] == -7, f"Cannibal Hymn at z={ch[2]}, expected -7"

    def test_book_of_dead_cosmological(self, full_resolver: SpatialResolver) -> None:
        bd = full_resolver.resolve("aeg_oraec_76")
        assert bd[2] == -6, f"Book of the Dead at z={bd[2]}, expected -6"

    def test_coffin_texts_funerary(self, full_resolver: SpatialResolver) -> None:
        ct = full_resolver.resolve("aeg_oraec_578")
        assert ct[2] == -4, f"Coffin Texts at z={ct[2]}, expected -4"

    def test_sinuhe_professional(self, full_resolver: SpatialResolver) -> None:
        s = full_resolver.resolve("aeg_oraec_17")
        assert s[2] == -1, f"Story of Sinuhe at z={s[2]}, expected -1"


# ------------------------------------------------------------------
# Room constraints
# ------------------------------------------------------------------

class TestRoomConstraints:
    def test_room_count_reasonable(self, full_state: dict) -> None:
        """Room count should be proportional to corpus size / max_per_room."""
        total = full_state.total_rooms
        min_rooms = len(full_state.room_assignments) // MAX_VOLUMES_PER_ROOM
        assert total >= min_rooms, f"Too few rooms: {total}"
        assert total < 10000, f"Too many rooms: {total}"

    def test_no_room_over_capacity(self, full_state: dict) -> None:
        """No room should exceed MAX_VOLUMES_PER_ROOM."""
        for room_key, volumes in full_state.room_volumes.items():
            assert len(volumes) <= MAX_VOLUMES_PER_ROOM, (
                f"Room {room_key} has {len(volumes)} volumes, max is {MAX_VOLUMES_PER_ROOM}"
            )

    def test_all_texts_assigned(self, full_state: dict, all_ids: list[str]) -> None:
        """Every input text must have a room assignment."""
        assert full_state.total_texts == len(all_ids)


# ------------------------------------------------------------------
# Index floors
# ------------------------------------------------------------------

class TestIndexFloors:
    def test_observatory_exists(self, full_state: dict) -> None:
        """There should be exactly one z=+3 room (Observatory)."""
        z3_rooms = [
            k for k in full_state.room_volumes
            if int(k.split(":")[2]) == 3
        ]
        assert len(z3_rooms) == 1, f"Expected 1 Observatory, got {len(z3_rooms)}"

    def test_district_galleries_exist(self, full_state: dict) -> None:
        """There should be rooms at z=+2 for district overviews."""
        z2_rooms = [
            k for k in full_state.room_volumes
            if int(k.split(":")[2]) == 2
        ]
        assert len(z2_rooms) >= 8, f"Expected >= 8 district galleries, got {len(z2_rooms)}"

    def test_cluster_atria_exist(self, full_state: dict) -> None:
        """There should be rooms at z=+1 for cluster summaries."""
        z1_rooms = [
            k for k in full_state.room_volumes
            if int(k.split(":")[2]) == 1
        ]
        assert len(z1_rooms) > 50, f"Expected > 50 cluster atria, got {len(z1_rooms)}"


# ------------------------------------------------------------------
# z-band computation
# ------------------------------------------------------------------

class TestZBands:
    def test_landmark_override(self) -> None:
        z = compute_z_band("aeg_oraec_502", 0.0, "unknown", {"aeg_oraec_502": -7})
        assert z == -7

    def test_physical_type_default(self) -> None:
        z = compute_z_band("test_123", 0.0, "coffin", {})
        assert z == -4

    def test_score_fallback(self) -> None:
        z = compute_z_band("test_123", 0.9, "unknown", {})
        assert z == -7

    def test_low_score_everyday(self) -> None:
        z = compute_z_band("test_123", -0.5, "unknown", {})
        assert z == 0


# ------------------------------------------------------------------
# Hex geometry
# ------------------------------------------------------------------

class TestHexGeometry:
    def test_hex_ring_zero(self) -> None:
        assert _hex_ring(0, 0, 0) == [(0, 0)]

    def test_hex_ring_one(self) -> None:
        ring = _hex_ring(0, 0, 1)
        assert len(ring) == 6

    def test_hex_ring_two(self) -> None:
        ring = _hex_ring(0, 0, 2)
        assert len(ring) == 12

    def test_hex_spiral_count(self) -> None:
        spiral = _hex_spiral_coords(0, 0, 3)
        # 1 + 6 + 12 + 18 = 37
        assert len(spiral) == 37

    def test_hex_distance_same(self) -> None:
        assert hex_distance((0, 0), (0, 0)) == 0

    def test_hex_distance_neighbor(self) -> None:
        assert hex_distance((0, 0), (1, 0)) == 1

    def test_hex_distance_far(self) -> None:
        assert hex_distance((0, 0), (3, -3)) == 3


# ------------------------------------------------------------------
# Persistence
# ------------------------------------------------------------------

class TestPersistence:
    def test_save_and_load(self, full_resolver: SpatialResolver, tmp_path: Path) -> None:
        out = tmp_path / "state.json"
        full_resolver.save_state(out)
        assert out.exists()

        r2 = SpatialResolver(CANONICAL_SEED, DATA_DIR)
        r2.load_state(out)
        assert r2.resolve("aeg_oraec_502") == full_resolver.resolve("aeg_oraec_502")
