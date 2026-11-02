"""BFS Unified Document Model — Pydantic v2 models used across the entire stack.

These models define the canonical internal representations for:
- Source records (AegyptusUDM)
- Content retrieval (SentenceRecord, VolumeContent)
- Search (SearchResult, SearchHit)
- Topology (DoorEdge, WallEdge, ShaftEdge, RoomMetadata, etc.)
"""

from __future__ import annotations

from pydantic import BaseModel, Field


# ---------------------------------------------------------------------------
# L2 — Unified Document Model
# ---------------------------------------------------------------------------

class AegyptusUDM(BaseModel):
    """A single projectable item from the Aegyptus corpus."""

    # Identity
    canonical_id: str
    content_hash: str  # SHA-256
    source_corpora: list[str]

    # Classification (drives spatial placement)
    physical_type: str
    ontological_score: float  # -1.0 to +1.0
    scribal_district: str

    # Content
    conventional_name: str | None
    script_type: str | None
    period_label: str | None
    sentence_count: int
    has_transliteration: bool
    has_translation: bool
    translation_languages: list[str]

    # Relational signals (for door/wall computation)
    concordance_systems: list[str]
    concordance_ids: list[str]
    cluster_id: str
    cluster_signature: list[str]

    # Embedding (optional for now)
    embedding_centroid: list[float] | None = None

    # Payload reference
    payload_ref: str

    # Provenance
    findspot: str | None
    current_location: str | None
    shelfmarks: list[str]
    date_range: tuple[int | None, int | None] | None = None


# ---------------------------------------------------------------------------
# Content retrieval
# ---------------------------------------------------------------------------

class SentenceRecord(BaseModel):
    """A single sentence from a volume."""

    id: int
    canonical_id: str
    sequence_num: int
    transliteration: str | None
    translation: dict[str, str]  # language code -> text
    tokens: list | None  # parsed token data
    source: str | None
    confidence: str | None


class VolumeContent(BaseModel):
    """Full readable content of a volume."""

    canonical_id: str
    conventional_name: str | None
    sentences: list[SentenceRecord]
    sentence_count: int
    has_transliteration: bool
    has_translation: bool
    translation_languages: list[str]


# ---------------------------------------------------------------------------
# Search
# ---------------------------------------------------------------------------

class SearchResult(BaseModel):
    """A search hit with score."""

    canonical_id: str
    conventional_name: str | None
    score: float
    snippet: str | None = None


class SearchHit(BaseModel):
    """Search result enriched with spatial info."""

    canonical_id: str
    conventional_name: str | None
    score: float
    snippet: str | None = None
    coord: tuple[int, int, int] | None = None
    district: str | None = None
    z_band: int | None = None


# ---------------------------------------------------------------------------
# Topology — shelf / volume references
# ---------------------------------------------------------------------------

class VolumeReference(BaseModel):
    """Reference to a volume on a shelf."""

    canonical_id: str
    conventional_name: str | None
    physical_type: str
    script_type: str | None


class ShelfData(BaseModel):
    """Data for a single shelf in a room."""

    index: int
    volumes: list[VolumeReference]


# ---------------------------------------------------------------------------
# Topology — edges
# ---------------------------------------------------------------------------

class DoorEdge(BaseModel):
    """A walkable connection between rooms."""

    from_coord: tuple[int, int, int]
    to_coord: tuple[int, int, int]
    direction: str  # N, NE, SE, S, SW, NW, UP, DOWN


class WallEdge(BaseModel):
    """An implicit non-walkable edge."""

    from_coord: tuple[int, int, int]
    to_coord: tuple[int, int, int]
    proximity_score: float = Field(ge=0.0, le=1.0)


class ShaftEdge(BaseModel):
    """A vertical connection between z-levels."""

    from_coord: tuple[int, int, int]
    to_coord: tuple[int, int, int]
    direction: str  # UP or DOWN


# ---------------------------------------------------------------------------
# Topology — room rendering
# ---------------------------------------------------------------------------

class RoomAmbient(BaseModel):
    """Aesthetic parameters for room rendering."""

    palette: str  # warm_amber, sage_green, ultramarine, phosphor_teal
    temperature: str  # hot, warm, cool, cold
    dust_level: float = Field(ge=0.0, le=1.0)
    era_material: str  # sandstone, granite, mudbrick, papyrus
    light_sources: int
    ceiling_height: str  # low, normal, cathedral, open


class RoomMetadata(BaseModel):
    """Everything the client needs to render a room."""

    codex: str  # [Q,R,Z]:[D1,D2]:[SEED]
    q: int
    r: int
    z: int
    room_type: str  # chamber, hallway, rotunda, vault, atrium, junction
    district: str
    door_edges: list[DoorEdge]
    wall_edges: list[WallEdge]
    shaft_edges: list[ShaftEdge]
    shelves: list[ShelfData]
    volume_count: int
    ambient: RoomAmbient
