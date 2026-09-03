-- ============================================================
-- EchoAI — Hierarchical chunks (migration 013)
-- Adds three-level structure: document → section → leaf.
-- See spec: 2026-05-15-tier3-hierarchical-retrieval-design.md
--
-- 2026-05-21 (feature 003): the unit_type CHECK list was widened in place to
-- add the legal-document levels (part/chapter/article/clause/subclause).
-- SQLite cannot ALTER a CHECK constraint; editing this list here is safe
-- because a schema-version bump nuclear-wipes echo.db and replays every
-- migration on a fresh DB (see schema.rs::maybe_wipe_for_schema_upgrade).
-- The original 'section'/'leaf' values are kept for Tier 3 backward compat.
-- ============================================================

ALTER TABLE chunks ADD COLUMN parent_chunk_id TEXT REFERENCES chunks(id);
ALTER TABLE chunks ADD COLUMN unit_type       TEXT NOT NULL DEFAULT 'leaf'
                                              CHECK (unit_type IN ('document','part','chapter','article','clause','subclause','section','leaf'));
ALTER TABLE chunks ADD COLUMN unit_label      TEXT;
ALTER TABLE chunks ADD COLUMN unit_path       TEXT;
ALTER TABLE chunks ADD COLUMN domain_metadata TEXT;

CREATE INDEX IF NOT EXISTS idx_chunks_parent     ON chunks(parent_chunk_id);
CREATE INDEX IF NOT EXISTS idx_chunks_unit_type  ON chunks(unit_type);
CREATE INDEX IF NOT EXISTS idx_chunks_unit_label ON chunks(unit_label);

INSERT INTO db_config(key, value) VALUES ('schema_version', '13')
    ON CONFLICT(key) DO UPDATE SET value = excluded.value;
