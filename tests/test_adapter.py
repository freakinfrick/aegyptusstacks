"""Integration tests for the Aegyptus source adapter (L1) and UDM (L2).

These tests hit the live Aegyptus PostgreSQL database.
"""

from __future__ import annotations

import pytest

from bfs.adapters.aegyptus import AegyptusAdapter
from bfs.udm.models import AegyptusUDM, SearchResult, SentenceRecord, VolumeContent

DB_URL = "postgresql://aegyptus:dev@localhost:5434/aegyptus"
CLASSIFICATIONS = "data/phase1/text_classifications.json"
CLUSTERS = "data/phase1/clusters.json"


@pytest.fixture(scope="module")
def adapter() -> AegyptusAdapter:
    return AegyptusAdapter(DB_URL, CLASSIFICATIONS, CLUSTERS)


# ---------------------------------------------------------------------------
# get_text
# ---------------------------------------------------------------------------


def test_load_single_text(adapter: AegyptusAdapter) -> None:
    udm = adapter.get_text("aeg_oraec_502")  # Pyramid Texts
    assert isinstance(udm, AegyptusUDM)
    assert udm.canonical_id == "aeg_oraec_502"
    assert udm.scribal_district  # has a district
    assert udm.content_hash  # non-empty hash
    assert udm.payload_ref.endswith("aeg_oraec_502")


def test_get_text_not_found(adapter: AegyptusAdapter) -> None:
    with pytest.raises(KeyError, match="not found"):
        adapter.get_text("nonexistent_id_xyz")


# ---------------------------------------------------------------------------
# enumerate
# ---------------------------------------------------------------------------


def test_enumerate_100(adapter: AegyptusAdapter) -> None:
    count = 0
    for udm in adapter.enumerate():
        assert isinstance(udm, AegyptusUDM)
        assert udm.canonical_id
        assert udm.physical_type
        count += 1
        if count >= 100:
            break
    assert count == 100


# ---------------------------------------------------------------------------
# get_sentences / get_volume_content
# ---------------------------------------------------------------------------


def test_get_sentences(adapter: AegyptusAdapter) -> None:
    sentences = adapter.get_sentences("aeg_oraec_502")
    assert len(sentences) > 0
    assert all(isinstance(s, SentenceRecord) for s in sentences)
    assert sentences[0].canonical_id == "aeg_oraec_502"
    # Pyramid Texts have transliteration
    assert any(s.transliteration is not None for s in sentences)


def test_get_volume_content(adapter: AegyptusAdapter) -> None:
    vc = adapter.get_volume_content("aeg_oraec_502")
    assert isinstance(vc, VolumeContent)
    assert vc.canonical_id == "aeg_oraec_502"
    assert vc.sentence_count > 0
    assert vc.sentence_count == len(vc.sentences)
    assert vc.has_transliteration is True


# ---------------------------------------------------------------------------
# search_lexical
# ---------------------------------------------------------------------------


def test_search_lexical(adapter: AegyptusAdapter) -> None:
    results = adapter.search_lexical("pyramid", limit=5)
    assert len(results) > 0
    assert all(isinstance(r, SearchResult) for r in results)
    assert all(r.canonical_id for r in results)


def test_search_lexical_transliteration(adapter: AegyptusAdapter) -> None:
    results = adapter.search_lexical("Rꜥ", fields=["transliteration"], limit=5)
    assert len(results) > 0


def test_search_lexical_no_results(adapter: AegyptusAdapter) -> None:
    results = adapter.search_lexical("xyzzyplughnotaword", limit=5)
    assert len(results) == 0
