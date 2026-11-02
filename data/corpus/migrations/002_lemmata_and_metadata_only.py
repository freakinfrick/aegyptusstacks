"""Add lemmata table, metadata_only completeness, source_corpus column.

Revision ID: 002
Revises: 001
Create Date: 2026-04-07
"""
from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import ARRAY, JSONB

revision: str = "002"
down_revision: Union[str, None] = "001"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    # ── Lemmata table ───────────────────────────────────────────

    op.create_table(
        "lemmata",
        sa.Column("id", sa.Text, primary_key=True),
        sa.Column("form", sa.Text, nullable=False),
        sa.Column("translation", JSONB),
        sa.Column("pos", sa.Text),
        sa.Column("language", sa.Text),
        sa.Column("source", sa.Text),
        sa.Column("frequency_rank", sa.Integer),
        sa.Column("extra", JSONB),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
    )

    op.create_index("idx_lemmata_form", "lemmata", ["form"])
    op.create_index("idx_lemmata_pos", "lemmata", ["pos"])
    op.create_index("idx_lemmata_source", "lemmata", ["source"])
    op.create_index("idx_lemmata_language", "lemmata", ["language"])

    # ── Update completeness CHECK to include 'metadata_only' ───

    op.drop_constraint("ck_texts_completeness", "texts", type_="check")
    op.create_check_constraint(
        "ck_texts_completeness",
        "texts",
        "completeness IN ('complete','fragmentary','damaged','unknown','metadata_only')",
    )

    # ── Add source_corpus array column ─────────────────────────

    op.add_column("texts", sa.Column("source_corpus", ARRAY(sa.Text)))

    # ── Backfill source_corpus from existing sentence data ─────

    op.execute("""
        UPDATE texts t SET source_corpus = sub.sources
        FROM (
            SELECT canonical_id, array_agg(DISTINCT source) as sources
            FROM sentences GROUP BY canonical_id
        ) sub
        WHERE t.canonical_id = sub.canonical_id AND t.source_corpus IS NULL
    """)


def downgrade() -> None:
    op.drop_column("texts", "source_corpus")

    op.drop_constraint("ck_texts_completeness", "texts", type_="check")
    op.create_check_constraint(
        "ck_texts_completeness",
        "texts",
        "completeness IN ('complete','fragmentary','damaged','unknown')",
    )

    op.drop_index("idx_lemmata_language", "lemmata")
    op.drop_index("idx_lemmata_source", "lemmata")
    op.drop_index("idx_lemmata_pos", "lemmata")
    op.drop_index("idx_lemmata_form", "lemmata")
    op.drop_table("lemmata")
