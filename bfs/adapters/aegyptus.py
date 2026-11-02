"""L1 Source Adapter — Aegyptus PostgreSQL corpus.

Connects to the Aegyptus database and emits normalized UDM records.
All Postgres-specific query logic lives here. No other BFS layer touches SQL.
"""

from __future__ import annotations

import hashlib
import json
from collections.abc import Iterator
from pathlib import Path
from typing import Any

from sqlalchemy import create_engine, text as sa_text
from sqlalchemy.engine import Engine

from bfs.udm.models import AegyptusUDM, SearchResult, SentenceRecord, VolumeContent

# Translation JSONB keys that are metadata, not real languages
_META_TRANSLATION_KEYS = frozenset({"_rosetta"})


# ---------------------------------------------------------------------------
# SQL templates
# ---------------------------------------------------------------------------

_SQL_ENUMERATE = """
SELECT
    t.canonical_id,
    t.conventional_name,
    t.script_type,
    t.period_label,
    t.text_type,
    t.findspot,
    t.current_location,
    t.shelfmarks,
    t.date_earliest,
    t.date_latest,
    t.source_corpus,
    COUNT(s.id) AS sentence_count,
    BOOL_OR(s.transliteration IS NOT NULL) AS has_transliteration,
    BOOL_OR(
        s.translation IS NOT NULL
        AND jsonb_typeof(s.translation) = 'object'
        AND s.translation != '{}'::jsonb
    ) AS has_translation,
    ARRAY_AGG(DISTINCT c.external_system)
        FILTER (WHERE c.external_system IS NOT NULL) AS concordance_systems,
    ARRAY_AGG(DISTINCT c.external_id)
        FILTER (WHERE c.external_id IS NOT NULL) AS concordance_ids
FROM texts t
LEFT JOIN sentences s ON s.canonical_id = t.canonical_id
LEFT JOIN concordance c ON c.canonical_id = t.canonical_id
GROUP BY t.canonical_id
"""

_SQL_SINGLE_TEXT = _SQL_ENUMERATE + "\nHAVING t.canonical_id = :canonical_id"

_SQL_SENTENCES = """
SELECT
    s.id,
    s.canonical_id,
    s.sequence_num,
    s.transliteration,
    s.translation,
    s.tokens,
    s.source,
    s.confidence
FROM sentences s
WHERE s.canonical_id = :canonical_id
ORDER BY s.sequence_num
"""

_SQL_TRANSLATION_LANGUAGES = """
SELECT DISTINCT jsonb_object_keys(s.translation) AS lang
FROM sentences s
WHERE s.canonical_id = :canonical_id
  AND s.translation IS NOT NULL
  AND jsonb_typeof(s.translation) = 'object'
  AND s.translation != '{}'::jsonb
"""

_SQL_BATCH_SENTENCES = """
SELECT
    s.canonical_id,
    s.sequence_num,
    s.transliteration,
    s.translation
FROM sentences s
WHERE s.canonical_id = ANY(:ids)
ORDER BY s.canonical_id, s.sequence_num
"""

_SQL_BATCH_NAMES = """
SELECT canonical_id, conventional_name
FROM texts
WHERE canonical_id = ANY(:ids)
"""

_SQL_SEARCH_TRANSLATION = """
SELECT
    s.canonical_id,
    t.conventional_name,
    s.translation->>'en' AS snippet
FROM sentences s
JOIN texts t ON t.canonical_id = s.canonical_id
WHERE s.translation IS NOT NULL
  AND jsonb_typeof(s.translation) = 'object'
  AND s.translation->>'en' ILIKE :pattern
LIMIT :limit
"""

_SQL_SEARCH_TRANSLITERATION = """
SELECT
    s.canonical_id,
    t.conventional_name,
    s.transliteration AS snippet
FROM sentences s
JOIN texts t ON t.canonical_id = s.canonical_id
WHERE s.transliteration ILIKE :pattern
LIMIT :limit
"""


class AegyptusAdapter:
    """Source adapter for the Aegyptus PostgreSQL corpus.

    Loads Phase 1 classification/cluster artifacts at init and joins them
    with live DB queries to produce fully-typed UDM records.
    """

    def __init__(
        self,
        db_url: str,
        classifications_path: str,
        clusters_path: str,
    ) -> None:
        self._engine: Engine = create_engine(db_url)
        self._classifications: dict[str, dict] = _load_json(classifications_path)
        self._clusters: dict[str, dict] = _load_json(clusters_path)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def enumerate(self) -> Iterator[AegyptusUDM]:
        """Yield every text in the corpus as a UDM record.

        Streams results via server-side cursor to avoid loading 77k rows
        into memory at once.
        """
        with self._engine.connect() as conn:
            result = conn.execution_options(stream_results=True).execute(
                sa_text(_SQL_ENUMERATE)
            )
            for row in result:
                udm = self._row_to_udm(row._mapping)
                if udm is not None:
                    yield udm

    def get_text(self, canonical_id: str) -> AegyptusUDM:
        """Fetch a single text's full UDM record."""
        with self._engine.connect() as conn:
            result = conn.execute(
                sa_text(_SQL_SINGLE_TEXT),
                {"canonical_id": canonical_id},
            )
            row = result.fetchone()
        if row is None:
            raise KeyError(f"Text not found: {canonical_id}")
        udm = self._row_to_udm(row._mapping)
        if udm is None:
            raise KeyError(f"Text could not be projected: {canonical_id}")
        return udm

    def get_sentences(self, canonical_id: str) -> list[SentenceRecord]:
        """All sentences for a text, ordered by sequence_num."""
        with self._engine.connect() as conn:
            result = conn.execute(
                sa_text(_SQL_SENTENCES),
                {"canonical_id": canonical_id},
            )
            rows = result.fetchall()
        return [_row_to_sentence(r._mapping) for r in rows]

    def get_volume_content(self, canonical_id: str) -> VolumeContent:
        """Full readable content for a text."""
        sentences = self.get_sentences(canonical_id)

        # Derive metadata from sentences
        has_translit = any(s.transliteration is not None for s in sentences)
        has_trans = any(bool(s.translation) for s in sentences)
        all_langs: set[str] = set()
        for s in sentences:
            all_langs.update(s.translation.keys())

        # Conventional name from DB
        with self._engine.connect() as conn:
            result = conn.execute(
                sa_text(
                    "SELECT conventional_name FROM texts WHERE canonical_id = :cid"
                ),
                {"cid": canonical_id},
            )
            row = result.fetchone()
        conv_name = row[0] if row else None

        return VolumeContent(
            canonical_id=canonical_id,
            conventional_name=conv_name,
            sentences=sentences,
            sentence_count=len(sentences),
            has_transliteration=has_translit,
            has_translation=has_trans,
            translation_languages=sorted(all_langs),
        )

    def get_batch_texts_compact(
        self, canonical_ids: list[str]
    ) -> list[dict[str, Any]]:
        """Batch-fetch texts with compact output (no tokens, no metadata).

        Returns a list of dicts, one per text, with only the fields an LLM
        needs to summarize or discuss the content:
          id, name, type, sentences: [{t, en}]

        Two SQL queries total regardless of volume count.
        """
        if not canonical_ids:
            return []

        with self._engine.connect() as conn:
            # 1. Batch-fetch names + physical type
            name_result = conn.execute(
                sa_text(_SQL_BATCH_NAMES), {"ids": canonical_ids}
            )
            names: dict[str, str | None] = {
                row.canonical_id: row.conventional_name for row in name_result
            }

            # 2. Batch-fetch all sentences (transliteration + translation only)
            sent_result = conn.execute(
                sa_text(_SQL_BATCH_SENTENCES), {"ids": canonical_ids}
            )
            from collections import defaultdict

            grouped: dict[str, list[dict[str, str | None]]] = defaultdict(list)
            for row in sent_result:
                trans_obj = row.translation
                if isinstance(trans_obj, str):
                    import json as _json

                    trans_obj = _json.loads(trans_obj)
                en = trans_obj.get("en") if isinstance(trans_obj, dict) else None
                grouped[row.canonical_id].append(
                    {"t": row.transliteration, "en": en}
                )

        # 3. Assemble compact output (preserve input order)
        results: list[dict[str, Any]] = []
        for cid in canonical_ids:
            results.append(
                {
                    "id": cid,
                    "name": names.get(cid),
                    "sentences": grouped.get(cid, []),
                }
            )
        return results

    def search_lexical(
        self,
        query: str,
        fields: list[str] | None = None,
        limit: int = 20,
    ) -> list[SearchResult]:
        """Full-text search over transliterations and translations.

        ``fields`` can contain ``"translation"`` and/or ``"transliteration"``.
        Defaults to both.
        """
        if fields is None:
            fields = ["translation", "transliteration"]

        pattern = f"%{query}%"
        seen: dict[str, SearchResult] = {}

        with self._engine.connect() as conn:
            if "translation" in fields:
                result = conn.execute(
                    sa_text(_SQL_SEARCH_TRANSLATION),
                    {"pattern": pattern, "limit": limit},
                )
                for row in result:
                    cid = row.canonical_id
                    if cid not in seen:
                        seen[cid] = SearchResult(
                            canonical_id=cid,
                            conventional_name=row.conventional_name,
                            score=1.0,
                            snippet=_truncate(row.snippet, 120),
                        )

            if "transliteration" in fields and len(seen) < limit:
                remaining = limit - len(seen)
                result = conn.execute(
                    sa_text(_SQL_SEARCH_TRANSLITERATION),
                    {"pattern": pattern, "limit": remaining},
                )
                for row in result:
                    cid = row.canonical_id
                    if cid not in seen:
                        seen[cid] = SearchResult(
                            canonical_id=cid,
                            conventional_name=row.conventional_name,
                            score=0.8,
                            snippet=_truncate(row.snippet, 120),
                        )

        return list(seen.values())[:limit]

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _row_to_udm(self, row: Any) -> AegyptusUDM | None:
        """Convert a DB row + Phase 1 data into an AegyptusUDM."""
        cid: str = row["canonical_id"]

        # Phase 1 classification
        cls = self._classifications.get(cid, {})
        physical_type = cls.get("physical_type", "unknown")
        district = cls.get("district", "unplaced")
        ontological_score = cls.get("ontological_score", 0.0)

        # Phase 1 cluster
        clu = self._clusters.get(cid, {})
        cluster_id = clu.get("cluster_id", "unclustered")
        cluster_sig = clu.get("cluster_signature_lemmata", [])

        # Source corpora
        source_corpora = row["source_corpus"] or []

        # Concordance arrays (may be None from ARRAY_AGG)
        concordance_systems = row["concordance_systems"] or []
        concordance_ids = row["concordance_ids"] or []

        # Content hash: deterministic identity from text metadata
        hash_input = f"{cid}:{row['sentence_count']}:{row['has_transliteration']}"
        content_hash = hashlib.sha256(hash_input.encode()).hexdigest()

        # Date range
        d_earliest = row.get("date_earliest")
        d_latest = row.get("date_latest")
        date_range = (d_earliest, d_latest) if (d_earliest or d_latest) else None

        # Shelfmarks
        shelfmarks = row["shelfmarks"] or []

        return AegyptusUDM(
            canonical_id=cid,
            content_hash=content_hash,
            source_corpora=source_corpora,
            physical_type=physical_type,
            ontological_score=ontological_score,
            scribal_district=district,
            conventional_name=row["conventional_name"],
            script_type=row["script_type"],
            period_label=row["period_label"],
            sentence_count=row["sentence_count"] or 0,
            has_transliteration=bool(row["has_transliteration"]),
            has_translation=bool(row["has_translation"]),
            translation_languages=_infer_languages(row["has_translation"]),
            concordance_systems=concordance_systems,
            concordance_ids=concordance_ids,
            cluster_id=cluster_id,
            cluster_signature=cluster_sig,
            payload_ref=f"postgres://aegyptus/texts/{cid}",
            findspot=row["findspot"],
            current_location=row["current_location"],
            shelfmarks=shelfmarks,
            date_range=date_range,
        )


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _load_json(path: str) -> dict:
    """Load a JSON file from disk."""
    return json.loads(Path(path).read_text())


def _is_error_sentinel(text: str) -> bool:
    """Detect synthetic error strings stored as corpus data."""
    return text.startswith("[ERROR") or text.startswith("ERROR:") or "Could not resolve" in text


def _row_to_sentence(row: Any) -> SentenceRecord:
    """Convert a sentence DB row into a SentenceRecord."""
    raw_translation = row["translation"]
    translation: dict[str, str] = {}
    if isinstance(raw_translation, dict):
        translation = {
            k: v
            for k, v in raw_translation.items()
            if k not in _META_TRANSLATION_KEYS and isinstance(v, str)
            and not _is_error_sentinel(v)
        }

    return SentenceRecord(
        id=row["id"],
        canonical_id=row["canonical_id"],
        sequence_num=row["sequence_num"] or 0,
        transliteration=row["transliteration"],
        translation=translation,
        tokens=row["tokens"],
        source=row["source"],
        confidence=row["confidence"],
    )


def _infer_languages(has_translation: bool) -> list[str]:
    """Infer translation languages from boolean flag.

    The corpus consistently has de + en when translations exist.
    Per-text language detection is done in get_volume_content().
    """
    if has_translation:
        return ["de", "en"]
    return []


def _truncate(s: str | None, max_len: int) -> str | None:
    """Truncate a string to max_len, adding ellipsis if needed."""
    if s is None:
        return None
    if len(s) <= max_len:
        return s
    return s[: max_len - 1] + "…"
