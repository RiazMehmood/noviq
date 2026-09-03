-- Feature 006 — Per-chunk progress persistence for the LLM IR builder.
--
-- Motivation: on 2026-05-30 the overnight import died from a lid-close suspend,
-- losing every chunk processed so far (chunks live only in-memory in
-- MistralIrBuilder::build until whole-doc validation succeeds). On 2026-05-31
-- a single-quote deterministic-retry loop on chunk n0008 burned 80 minutes
-- and produced the same loss pattern. This table lets the builder resume
-- from the next un-built chunk on app restart or after a transient stall.
--
-- Additive only — never drops or modifies prior columns (Constitution constraint).
-- One row per source while a build is in flight. Deleted on successful
-- completion. Mismatched `total_chunks` (e.g. source text changed) → ignored
-- and the build restarts from chunk 0.

CREATE TABLE IF NOT EXISTS doc_ir_progress (
    source_id       TEXT PRIMARY KEY,
    source_sha256   TEXT NOT NULL,
    doc_type        TEXT NOT NULL,        -- one of DocType::as_str()
    total_chunks    INTEGER NOT NULL,     -- chunker output cardinality
    next_chunk_idx  INTEGER NOT NULL,     -- 0..=total_chunks; total_chunks means "done, awaiting validation"
    id_counter      INTEGER NOT NULL,     -- doc-global node-id counter at last save
    nodes_json      TEXT NOT NULL,        -- JSON array of StructureNode rows accumulated so far
    updated_at      INTEGER NOT NULL      -- unix ms
);

CREATE INDEX IF NOT EXISTS idx_doc_ir_progress_sha
    ON doc_ir_progress(source_sha256);

INSERT INTO db_config(key, value) VALUES ('schema_version', '15')
    ON CONFLICT(key) DO UPDATE SET value = excluded.value;
