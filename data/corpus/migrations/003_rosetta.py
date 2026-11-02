"""Add thoth_lexicon and rosetta_lemma_index tables.

Revision ID: 003
Revises: 002
Create Date: 2026-04-07
"""
from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import JSONB

revision: str = "003"
down_revision: Union[str, None] = "002"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    # ── Thoth lexicon table ────────────────────────────────────

    op.create_table(
        "thoth_lexicon",
        sa.Column("lemma_id", sa.Text, sa.ForeignKey("lemmata.id"), primary_key=True),
        sa.Column("definition_de", sa.Text),
        sa.Column("definition_en", sa.Text),
        sa.Column("glosses_de", JSONB),
        sa.Column("glosses_en", JSONB),
        sa.Column("glosses_bridged_en", JSONB),
        sa.Column("coptic_cognates", JSONB),
        sa.Column("hieroglyphs", JSONB),
        sa.Column("pos", sa.Text),
        sa.Column("pos_distribution", JSONB),
        sa.Column("frequency", sa.Integer),
        sa.Column("frequency_rank", sa.Integer),
        sa.Column("period_range", JSONB),
        sa.Column("text_types", JSONB),
        sa.Column("examples", JSONB),
        sa.Column("sources", JSONB),
        sa.Column("confidence", sa.Text),
        sa.Column("updated_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
    )

    op.create_index("idx_thoth_frequency", "thoth_lexicon", ["frequency"], postgresql_ops={"frequency": "DESC"})
    op.create_index("idx_thoth_pos", "thoth_lexicon", ["pos"])
    op.create_index("idx_thoth_glosses_en", "thoth_lexicon", ["glosses_en"], postgresql_using="gin")
    op.create_index("idx_thoth_confidence", "thoth_lexicon", ["confidence"])

    # ── Rosetta lemma index table ──────────────────────────────

    op.create_table(
        "rosetta_lemma_index",
        sa.Column("lemma_id", sa.Text, nullable=False),
        sa.Column("sentence_id", sa.Integer, sa.ForeignKey("sentences.id"), nullable=False),
        sa.PrimaryKeyConstraint("lemma_id", "sentence_id"),
    )

    op.create_index("idx_rosetta_lemma_sent", "rosetta_lemma_index", ["sentence_id"])


def downgrade() -> None:
    op.drop_index("idx_rosetta_lemma_sent", "rosetta_lemma_index")
    op.drop_table("rosetta_lemma_index")

    op.drop_index("idx_thoth_confidence", "thoth_lexicon")
    op.drop_index("idx_thoth_glosses_en", "thoth_lexicon")
    op.drop_index("idx_thoth_pos", "thoth_lexicon")
    op.drop_index("idx_thoth_frequency", "thoth_lexicon")
    op.drop_table("thoth_lexicon")
