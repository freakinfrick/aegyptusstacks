#!/usr/bin/env python3
"""Build the read-only SQLite file the browser Stacks reads (no server).

Everything the BFS server computes at startup is precomputed here: room
metadata (with doors, walls, shafts) comes from the real BFS services, so the
browser sees exactly what `get_room` would return. Text comes from Postgres.

    python3 tools/export_web_db.py build  --out web/stacks.sqlite
    python3 tools/export_web_db.py licenses --db web/stacks.sqlite > DATA-LICENSES.md
    python3 tools/export_web_db.py check  --db web/stacks.sqlite --licenses DATA-LICENSES.md

Licence rules (see DATA-LICENSES.md, generated from the file itself):
  - Nederhof PhilologEg is left out (redistribution needs the author's permission).
  - AES sentence rows are left out: unordered near-duplicates of ORAEC rows.
  - Test fixtures (ad_hoc, test, test_suite) are left out.
  - RAMSES rows are kept and flagged non-commercial per sentence.
"""
from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import os
import sqlite3
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DSN = os.environ.get("DATABASE_URL", "postgresql://aegyptus:dev@localhost:5434/aegyptus")
PAGE_SIZE = int(os.environ.get("EXPORT_PAGE_SIZE", "4096"))

# Source key -> (display name, licence, non-commercial, url). Licences as recorded
# in the aegyptusdb README source list.
SOURCES = {
    "tla_hf": ("Thesaurus Linguae Aegyptiae (Hugging Face)", "CC BY-SA 4.0", 0,
               "https://huggingface.co/thesaurus-linguae-aegyptiae"),
    "oraec": ("ORAEC (Simon D. Schweitzer, BBAW)", "CC BY-SA 4.0", 0,
              "https://github.com/oraec/corpus_raw_data"),
    "aed_tei": ("AED-TEI (Simon D. Schweitzer, BBAW)", "CC BY-SA 4.0", 0,
                "https://github.com/simondschweitzer/aed-tei"),
    "coptic_scriptorium": ("Coptic Scriptorium", "CC BY 4.0", 0,
                           "https://copticscriptorium.org"),
    "ramses": ("Ramsès Late Egyptian Corpus (Université de Liège)", "CC BY-NC-SA 4.0", 1,
               "https://ramses.ulg.ac.be"),
    "met_api": ("The Metropolitan Museum of Art Open Access", "CC0", 0,
                "https://www.metmuseum.org/about-the-met/policies-and-documents/open-access"),
    "papyri.info": ("papyri.info (DDbDP / DCLP / IDP)", "CC BY 3.0", 0, "https://papyri.info"),
    "nederhof_hierojax": ("Nederhof HieroJax (names only, no text)", "names only", 0,
                          "https://nederhof.github.io/hierojax/"),
    "manual": ("Aegyptus manual entries", "public domain", 0, ""),
}
EXCLUDED = {
    "nederhof_philologeg": "redistribution needs the author's permission",
    "aes": "unordered near-duplicate of ORAEC rows (97,975 of 101,796 identical)",
    "ad_hoc": "test fixture",
    "test": "test fixture",
    "test_suite": "test fixture",
}

SCHEMA = """
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT) WITHOUT ROWID;
CREATE TABLE sources(source TEXT PRIMARY KEY, name TEXT, licence TEXT, nc INT,
  url TEXT, texts INT, sentences INT) WITHOUT ROWID;
CREATE TABLE texts(canonical_id TEXT PRIMARY KEY, name TEXT, script TEXT, period TEXT,
  text_type TEXT, findspot TEXT, location TEXT, date_earliest INT, date_latest INT,
  source TEXT, room_key TEXT, sentence_count INT, nc INT) WITHOUT ROWID;
CREATE TABLE sentences(id INTEGER PRIMARY KEY, canonical_id TEXT, seq INT, source TEXT,
  translit TEXT, en TEXT, de TEXT, nc INT,
  en_ml TEXT);  -- en_ml: model that machine-made the English, NULL = from the source
CREATE TABLE rooms(room_key TEXT PRIMARY KEY, q INT, r INT, z INT, json TEXT) WITHOUT ROWID;
"""


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", file=sys.stderr, flush=True)


def text_source(source_corpus: list[str] | None, metadata: dict | None) -> str:
    """One source key per text: first corpus tag, else the metadata's own source."""
    if source_corpus:
        # 'aes,oraec' texts are ORAEC records; AES only contributes sentences.
        tags = [s for s in source_corpus if s != "aes"] or list(source_corpus)
        return tags[0]
    return (metadata or {}).get("source") or "unrecorded"


def load_bfs():
    """Boot the BFS services in-process exactly as bfs/server.py's lifespan does."""
    sys.path.insert(0, str(ROOT))
    from bfs.adapters.aegyptus import AegyptusAdapter
    from bfs.index.service import IndexService
    from bfs.resolver.resolver import SpatialResolver
    from bfs.topology.graph import TopologyGraph

    data_dir = ROOT / "data" / "phase1"
    adapter = AegyptusAdapter(
        db_url=DSN,
        classifications_path=str(data_dir / "text_classifications.json"),
        clusters_path=str(data_dir / "clusters.json"),
    )
    resolver = SpatialResolver(data_dir=str(data_dir))
    resolver.load_state(ROOT / "data" / "topology" / "resolved_topology.json")
    graph = TopologyGraph()
    graph.build_from_resolver_state(resolver._state)
    for key, meta in resolver._state.room_metadata.items():
        resolver._room_metadata.setdefault(key, meta)
    return resolver, IndexService(adapter=adapter, graph=graph, resolver=resolver)


def build(out: Path) -> None:
    import psycopg2

    t0 = time.time()
    tmp = out.with_suffix(".tmp")
    tmp.unlink(missing_ok=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(tmp)
    db.execute(f"PRAGMA page_size={PAGE_SIZE}")
    db.executescript("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;" + SCHEMA)
    pg = psycopg2.connect(DSN.replace("postgresql://", "postgres://"))

    # 1. Texts (text-level exclusions decided here).
    cur = pg.cursor()
    cur.execute("""select canonical_id, conventional_name, script_type, period_label, text_type,
        findspot, current_location, date_earliest, date_latest, source_corpus, metadata from texts""")
    kept: dict[str, str] = {}
    rows = []
    for (cid, name, script, period, ttype, find, loc, de, dl, corpus, md) in cur:
        src = text_source(corpus, md)
        if src in EXCLUDED or (corpus and set(corpus) & set(EXCLUDED) - {"aes"}):
            continue
        kept[cid] = src
        rows.append((cid, html.unescape(name) if name else name, script, period, ttype, find, loc, de, dl, src))
    db.executemany("""insert into texts(canonical_id, name, script, period, text_type, findspot,
        location, date_earliest, date_latest, source) values (?,?,?,?,?,?,?,?,?,?)""", rows)
    log(f"texts kept {len(kept)}")

    # 2. Sentences (row-level exclusions; licence flag per row).
    cur = pg.cursor(name="sentences")
    cur.itersize = 20000
    cur.execute("""select id, canonical_id, sequence_num, source, transliteration,
        translation->>'en', translation->>'de',
        case when source_detail ? 'en_translation'
             then coalesce(source_detail->'en_translation'->>'model', 'unknown') end
        from sentences order by canonical_id,
        sequence_num nulls last, id""")
    n = skipped = 0
    while batch := cur.fetchmany(20000):
        good = [(*r[:7], SOURCES.get(r[3], ("", "", 0, ""))[2], *r[7:]) for r in batch
                if r[3] not in EXCLUDED and r[1] in kept]
        skipped += len(batch) - len(good)
        db.executemany("insert into sentences values (?,?,?,?,?,?,?,?,?)", good)
        n += len(good)
    log(f"sentences kept {n}, left out {skipped}")
    unknown = db.execute("select distinct source from sentences").fetchall()
    unknown = {s for (s,) in unknown} - set(SOURCES)
    unknown |= {s for s in kept.values()} - set(SOURCES)
    if unknown:
        sys.exit(f"sources with no licence entry: {sorted(unknown)} (add them to SOURCES)")
    db.execute("create index s_text on sentences(canonical_id, seq)")
    db.execute("""update texts set
        sentence_count = (select count(*) from sentences s where s.canonical_id = texts.canonical_id),
        nc = coalesce((select max(nc) from sentences s where s.canonical_id = texts.canonical_id), 0)""")

    # 3. Rooms, straight from the BFS services; excluded texts taken off the shelves.
    resolver, index = load_bfs()
    room_rows = []
    for key in resolver._room_metadata:
        q, r, z = (int(v) for v in key.split(":"))
        meta = index.get_room_metadata(q, r, z)
        meta["volumes"] = [v for v in meta["volumes"] if v in kept]
        meta["volumes_typed"] = [v for v in meta["volumes_typed"] if v["id"] in kept]
        meta["shelves"] = [[v for v in s if v in kept] for s in meta.get("shelves", [])]
        meta["volume_count"] = len(meta["volumes"])
        room_rows.append((key, q, r, z, json.dumps(meta, ensure_ascii=False, separators=(",", ":"))))
    # A text can stand on shelves in several rooms; its home is the resolver's assignment.
    placed = [(":".join(map(str, c)), cid) for cid, c in resolver._resolved.items() if cid in kept]
    db.executemany("insert into rooms values (?,?,?,?,?)", room_rows)
    db.executemany("update texts set room_key = ? where canonical_id = ?", placed)
    log(f"rooms {len(room_rows)}, placed texts {len(placed)}")

    # 4. Source table, meta, full-text index.
    for src, (name, lic, nc, url) in SOURCES.items():
        t = db.execute("select count(*) from texts where source=?", (src,)).fetchone()[0]
        s = db.execute("select count(*) from sentences where source=?", (src,)).fetchone()[0]
        db.execute("insert into sources values (?,?,?,?,?,?,?)", (src, name, lic, nc, url, t, s))
    meta = {
        "built": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d"),
        "excluded": json.dumps(EXCLUDED),
        "page_size": str(PAGE_SIZE),
    }
    db.executemany("insert into meta values (?,?)", meta.items())
    db.executescript("""
    CREATE VIRTUAL TABLE fts USING fts5(translit, en, de, content='sentences', content_rowid='id',
      tokenize='unicode61 remove_diacritics 2');
    INSERT INTO fts(fts) VALUES('rebuild');
    INSERT INTO fts(fts) VALUES('optimize');
    """)
    db.commit()
    db.execute("vacuum")
    db.close()
    tmp.replace(out)
    log(f"wrote {out} ({out.stat().st_size / 1e6:.0f} MB) in {time.time() - t0:.0f}s")


def licenses_md(db_path: Path) -> str:
    db = sqlite3.connect(db_path)
    built = db.execute("select value from meta where key='built'").fetchone()[0]
    excluded = json.loads(db.execute("select value from meta where key='excluded'").fetchone()[0])
    lines = [
        "# Data licences: The Stacks web edition",
        "",
        "The code is MIT. The texts are not ours: each source keeps its own licence, listed below",
        "with the exact number of records this edition carries from it. This page is generated from",
        f"the data file itself (`tools/export_web_db.py licenses`). File built {built}.",
        "",
        "## Included",
        "",
    ]
    for src, name, lic, nc, url, t, s in db.execute(
            "select * from sources where texts > 0 or sentences > 0 order by sentences desc, texts desc"):
        flag = " **Non-commercial use only.**" if nc else ""
        link = f" <{url}>" if url else ""
        lines.append(f"- **{name}**: {lic}.{flag} {t:,} texts, {s:,} sentences.{link}")
    nc_texts = db.execute("select count(*) from texts where nc = 1").fetchone()[0]
    lines += [
        "",
        f"{nc_texts:,} texts hold RAMSES sentences. Every RAMSES sentence carries the non-commercial",
        "flag in the file and on the page. RAMSES arrives as its dataset splits (train/test/val),",
        "not as separate texts, so each split is one very long volume.",
        "",
        "## Machine-made English",
        "",
        "Most English here is a machine translation (`ml_draft`, needs review), made from the source's",
        "German or from the transliteration. Each such sentence is marked on the page, with the model",
        "named and the source's own German beside it where there is one. Machine English per source:",
        "",
    ]
    for src, name, total, ml in db.execute("""select s.source, x.name, count(*),
            sum(s.en_ml is not null) from sentences s join sources x using(source)
            where s.en is not null and s.en <> '' group by 1 order by 3 desc"""):
        lines.append(f"- {name}: {ml:,} of {total:,} English sentences machine-made.")
    lines += ["", "By model:", ""]
    for model, n in db.execute("""select en_ml, count(*) from sentences where en_ml is not null
            group by 1 order by 2 desc"""):
        lines.append(f"- `{model}`: {n:,}")
    lines += [
        "",
        "## Left out",
        "",
    ]
    lines += [f"- `{k}`: {v}." for k, v in excluded.items()]
    lines += ["", "Concordance links (Trismegistos and others) are not in this edition.", ""]
    return "\n".join(lines)


def check(db_path: Path, licenses: Path) -> int:
    db = sqlite3.connect(db_path)
    problems = []
    if licenses.read_text() != licenses_md(db_path):
        problems.append(f"{licenses} does not match the file; regenerate it with `licenses`")
    for src in EXCLUDED:
        n = db.execute("select count(*) from sentences where source=?", (src,)).fetchone()[0]
        n += db.execute("select count(*) from texts where source=?", (src,)).fetchone()[0]
        if n:
            problems.append(f"excluded source {src} has {n} rows")
    bad_nc = db.execute("select count(*) from sentences where source='ramses' and nc != 1").fetchone()[0]
    if bad_nc:
        problems.append(f"{bad_nc} RAMSES rows without the NC flag")
    unplaced = db.execute("select count(*) from texts where room_key is null").fetchone()[0]
    print(f"texts {db.execute('select count(*) from texts').fetchone()[0]:,} "
          f"(unplaced {unplaced:,}), sentences {db.execute('select count(*) from sentences').fetchone()[0]:,}, "
          f"rooms {db.execute('select count(*) from rooms').fetchone()[0]:,}, "
          f"size {db_path.stat().st_size / 1e6:.0f} MB")
    for row in db.execute("select source, licence, nc, texts, sentences from sources order by sentences desc"):
        print("  ", *row)
    for p in problems:
        print("PROBLEM:", p)
    return 1 if problems else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--out", type=Path, default=ROOT / "web" / "stacks.sqlite")
    lic = sub.add_parser("licenses")
    lic.add_argument("--db", type=Path, default=ROOT / "web" / "stacks.sqlite")
    c = sub.add_parser("check")
    c.add_argument("--db", type=Path, default=ROOT / "web" / "stacks.sqlite")
    c.add_argument("--licenses", type=Path, default=ROOT / "DATA-LICENSES.md")
    a = ap.parse_args()
    if a.cmd == "build":
        build(a.out)
    elif a.cmd == "licenses":
        sys.stdout.write(licenses_md(a.db))
    else:
        return check(a.db, a.licenses)
    return 0


if __name__ == "__main__":
    sys.exit(main())
