"""Initial schema: all tables and indexes from spec Appendix B.

Revision ID: 001
Revises: None
Create Date: 2026-04-01
"""
from typing import Sequence, Union

import sqlalchemy as sa
from alembic import op
from sqlalchemy.dialects.postgresql import ARRAY, JSONB

revision: str = "001"
down_revision: Union[str, None] = None
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    # ── Core corpus tables ──────────────────────────────────────

    op.create_table(
        "texts",
        sa.Column("canonical_id", sa.Text, primary_key=True),
        sa.Column("conventional_name", sa.Text),
        sa.Column("script_type", sa.Text),
        sa.Column("date_earliest", sa.Integer),
        sa.Column("date_latest", sa.Integer),
        sa.Column("period_label", sa.Text),
        sa.Column("findspot", sa.Text),
        sa.Column("current_location", sa.Text),
        sa.Column("text_type", sa.Text),
        sa.Column("completeness", sa.Text),
        sa.Column("shelfmarks", ARRAY(sa.Text)),
        sa.Column("metadata", JSONB),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
        sa.Column("updated_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
        sa.CheckConstraint(
            "script_type IN ('hieroglyphic','hieratic','demotic','coptic','greek','multiple')",
            name="ck_texts_script_type",
        ),
        sa.CheckConstraint(
            "completeness IN ('complete','fragmentary','damaged','unknown')",
            name="ck_texts_completeness",
        ),
    )

    op.create_table(
        "sentences",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("canonical_id", sa.Text, sa.ForeignKey("texts.canonical_id"), nullable=False),
        sa.Column("sequence_num", sa.Integer),
        sa.Column("transliteration", sa.Text),
        sa.Column("translation", JSONB),
        sa.Column("tokens", JSONB),
        sa.Column("source", sa.Text),
        sa.Column("source_id", sa.Text),
        sa.Column("source_detail", JSONB),
        sa.Column("confidence", sa.Text, server_default="verified"),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
    )

    op.create_table(
        "concordance",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("canonical_id", sa.Text, sa.ForeignKey("texts.canonical_id"), nullable=False),
        sa.Column("external_system", sa.Text, nullable=False),
        sa.Column("external_id", sa.Text, nullable=False),
        sa.Column("relationship", sa.Text, server_default="exact_match"),
        sa.UniqueConstraint("external_system", "external_id", "canonical_id", name="uq_concordance_system_ext_canonical"),
    )

    op.create_table(
        "aliases",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("canonical_id", sa.Text, sa.ForeignKey("texts.canonical_id"), nullable=False),
        sa.Column("alias_text", sa.Text, nullable=False),
        sa.Column("alias_normalized", sa.Text, nullable=False),
        sa.Column("alias_type", sa.Text, nullable=False),
        sa.Column("confidence", sa.Text, server_default="auto"),
        sa.UniqueConstraint("alias_text", "canonical_id", name="uq_aliases_text_canonical"),
    )

    op.create_table(
        "representation_layers",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("canonical_id", sa.Text, sa.ForeignKey("texts.canonical_id"), nullable=False),
        sa.Column("layer_type", sa.Text, nullable=False),
        sa.Column("content", JSONB),
        sa.Column("language", sa.Text),
        sa.Column("source", sa.Text),
        sa.Column("source_detail", JSONB),
        sa.Column("confidence", sa.Text, server_default="verified"),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
    )

    # ── Pipeline tables ─────────────────────────────────────────

    op.create_table(
        "crawl_queue",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("url", sa.Text, unique=True, nullable=False),
        sa.Column("domain", sa.Text, nullable=False),
        sa.Column("content_type_hint", sa.Text),
        sa.Column("status", sa.Text, server_default="pending"),
        sa.Column("fetched_at", sa.DateTime(timezone=True)),
        sa.Column("object_store_key", sa.Text),
        sa.Column("metadata", JSONB),
    )

    op.create_table(
        "gaps",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("canonical_id", sa.Text, sa.ForeignKey("texts.canonical_id")),
        sa.Column("external_system", sa.Text, nullable=False),
        sa.Column("external_id", sa.Text, nullable=False),
        sa.Column("gap_type", sa.Text, nullable=False),
        sa.Column("priority", sa.Text, nullable=False),
        sa.Column("recommended_action", sa.Text),
        sa.Column("status", sa.Text, server_default="open"),
        sa.Column("assigned_to", sa.Text),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
    )

    # ── Operations tables ───────────────────────────────────────

    op.create_table(
        "corpus_versions",
        sa.Column("id", sa.Integer, primary_key=True, autoincrement=True),
        sa.Column("version_tag", sa.Text, unique=True, nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.func.now()),
        sa.Column("description", sa.Text),
        sa.Column("text_count", sa.Integer),
        sa.Column("sentence_count", sa.Integer),
        sa.Column("layer_count", sa.Integer),
        sa.Column("concordance_count", sa.Integer),
        sa.Column("alias_count", sa.Integer),
        sa.Column("export_key", sa.Text),
        sa.Column("metadata", JSONB),
    )

    # ── Indexes ─────────────────────────────────────────────────

    # Concordance lookups (both directions)
    op.create_index("idx_concordance_canonical", "concordance", ["canonical_id"])
    op.create_index("idx_concordance_external", "concordance", ["external_system", "external_id"])

    # Sentence queries
    op.create_index("idx_sentences_canonical", "sentences", ["canonical_id"])
    op.create_index("idx_sentences_source", "sentences", ["source"])
    op.create_index("idx_sentences_confidence", "sentences", ["confidence"])
    op.create_index("idx_sentences_tokens", "sentences", ["tokens"], postgresql_using="gin")

    # Alias matching
    op.create_index("idx_aliases_normalized", "aliases", ["alias_normalized"])
    op.create_index("idx_aliases_canonical", "aliases", ["canonical_id"])

    # Representation layer queries
    op.create_index("idx_layers_canonical", "representation_layers", ["canonical_id"])
    op.create_index("idx_layers_type", "representation_layers", ["layer_type"])
    op.create_index("idx_layers_language", "representation_layers", ["language"])

    # Text filtering
    op.create_index("idx_texts_script", "texts", ["script_type"])
    op.create_index("idx_texts_period", "texts", ["period_label"])
    op.create_index("idx_texts_type", "texts", ["text_type"])

    # Gap tracker
    op.create_index("idx_gaps_priority", "gaps", ["priority", "status"])
    op.create_index("idx_gaps_canonical", "gaps", ["canonical_id"])


def downgrade() -> None:
    # Drop indexes (implicit with table drop, but explicit for clarity)
    op.drop_index("idx_gaps_canonical", table_name="gaps")
    op.drop_index("idx_gaps_priority", table_name="gaps")
    op.drop_index("idx_texts_type", table_name="texts")
    op.drop_index("idx_texts_period", table_name="texts")
    op.drop_index("idx_texts_script", table_name="texts")
    op.drop_index("idx_layers_language", table_name="representation_layers")
    op.drop_index("idx_layers_type", table_name="representation_layers")
    op.drop_index("idx_layers_canonical", table_name="representation_layers")
    op.drop_index("idx_aliases_canonical", table_name="aliases")
    op.drop_index("idx_aliases_normalized", table_name="aliases")
    op.drop_index("idx_sentences_tokens", table_name="sentences")
    op.drop_index("idx_sentences_confidence", table_name="sentences")
    op.drop_index("idx_sentences_source", table_name="sentences")
    op.drop_index("idx_sentences_canonical", table_name="sentences")
    op.drop_index("idx_concordance_external", table_name="concordance")
    op.drop_index("idx_concordance_canonical", table_name="concordance")

    # Drop tables in reverse dependency order
    op.drop_table("corpus_versions")
    op.drop_table("gaps")
    op.drop_table("crawl_queue")
    op.drop_table("representation_layers")
    op.drop_table("aliases")
    op.drop_table("concordance")
    op.drop_table("sentences")
    op.drop_table("texts")
