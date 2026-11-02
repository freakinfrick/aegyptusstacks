"""Initialize the BFS topology by resolving the entire corpus.

Standalone script that runs the full resolution pipeline:
1. Connect to Aegyptus DB via adapter
2. Get all canonical IDs
3. Create resolver with canonical seed
4. resolve_all()
5. Build topology graph
6. Save state
7. Print statistics

Usage:
    python -m bfs.resolver.initialize \
        --db-url postgresql://aegyptus:dev@localhost:5434/aegyptus \
        --data-dir data/phase1 \
        --output-dir bfs/data
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description="Initialize BFS topology")
    parser.add_argument(
        "--db-url",
        default="postgresql://aegyptus:dev@localhost:5434/aegyptus",
        help="Aegyptus database URL",
    )
    parser.add_argument(
        "--data-dir",
        default="data/phase1",
        help="Path to Phase 1 data directory (classifications, clusters, landmarks)",
    )
    parser.add_argument(
        "--output-dir",
        default="bfs/data",
        help="Output directory for resolved state files",
    )
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    # 1. Get all canonical IDs from the classifications file
    classifications_path = Path(args.data_dir) / "text_classifications.json"
    with open(classifications_path) as f:
        classifications = json.load(f)
    canonical_ids = list(classifications.keys())
    print(f"Found {len(canonical_ids):,} texts in classifications")

    # 2. Fetch period_labels from Aegyptus DB for chronological z-axis
    print("Fetching period labels from Aegyptus DB...")
    period_labels: dict[str, str] = {}
    try:
        from sqlalchemy import create_engine, text as sql_text
        engine = create_engine(args.db_url)
        with engine.connect() as conn:
            rows = conn.execute(sql_text(
                "SELECT canonical_id, period_label FROM texts WHERE period_label IS NOT NULL"
            ))
            for row in rows:
                period_labels[row[0]] = row[1]
        print(f"  Loaded {len(period_labels):,} period labels")
    except Exception as e:
        print(f"  WARNING: Could not load period labels: {e}")
        print(f"  Falling back to z=0 (undated) for all texts")

    # 3. Create resolver with canonical seed
    from bfs.resolver.constants import CANONICAL_SEED
    from bfs.resolver.resolver import SpatialResolver

    resolver = SpatialResolver(seed=CANONICAL_SEED, data_dir=args.data_dir)
    resolver._period_labels = period_labels  # inject period data for chronological z-axis

    # 4. resolve_all()
    print(f"Resolving {len(canonical_ids):,} texts...")
    t0 = time.perf_counter()
    state = resolver.resolve_all(canonical_ids)
    t_resolve = time.perf_counter() - t0

    # 5. Build topology graph
    print("Building topology graph...")
    from bfs.topology.graph import TopologyGraph

    graph = TopologyGraph()
    graph.build_from_resolver_state(state)

    # Enforce two-door constraint
    removed = graph.enforce_two_door_constraint()
    if removed:
        print(f"  Removed {removed} excess door edges (two-door constraint)")

    graph_stats = graph.stats()

    # 6. Save state
    state_path = output_dir / "resolved_topology.json"
    resolver.save_state(state_path)
    print(f"State saved to {state_path}")

    # 7. Print statistics
    print()
    print("=" * 60)
    print("BFS Resolution Complete")
    print("=" * 60)
    print(f"  Total texts resolved: {state.total_texts:,}")
    print(f"  Total rooms created:  {state.total_rooms:,}")
    print()

    print("  Districts:")
    for district, count in sorted(state.district_stats.items(), key=lambda x: -x[1]):
        print(f"    {district:20s} = {count:,}")

    print()
    print("  Z-bands:")
    for z_str, count in sorted(state.z_band_stats.items(), key=lambda x: int(x[0])):
        print(f"    z={z_str:>3s}: {count:,}")

    print()
    print("  Topology:")
    print(f"    Door edges:       {graph_stats['total_doors']:,}")
    print(f"    Wall edges:       {graph_stats['total_walls']:,}")
    print(f"    Shaft edges:      {graph_stats['total_shafts']:,}")
    print(f"    Components:       {graph_stats['connected_components']:,}")

    print()
    print(f"  Resolution time: {t_resolve:.1f}s")
    print()


if __name__ == "__main__":
    main()
