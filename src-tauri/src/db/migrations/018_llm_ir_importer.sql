-- Feature 006 — LLM PDF→IR importer.
-- Additive-only (Constitution development constraint).
--
-- NOTE: `extraction_method` and `quality_score` were already added to `sources`
-- by migration 016 (OCR text-quality). We DO NOT re-add them here.
-- Migration 016's `extraction_method` carries the existing values
-- (`'lopdf'`/`'pdftotext'`/`'tesseract'`); feature 006 extends the enumerated
-- vocabulary in-place with `'llm_ir'` / `'rules_legacy'` / `'constitution_ir'` /
-- `'unprocessable'` — no schema change required, just a documented value set.

-- Cache index — one row per cached IR file on disk at
-- {app_data_dir}/ir_cache/<sha256>.json.
CREATE TABLE IF NOT EXISTS ir_cache_index (
    sha256                  TEXT PRIMARY KEY,             -- = DocIr.doc_id
    schema_version          INTEGER NOT NULL,
    doc_type                TEXT NOT NULL,
    nodes_total             INTEGER NOT NULL,
    nodes_ok                INTEGER NOT NULL,
    nodes_low_confidence    INTEGER NOT NULL,
    nodes_user_repaired     INTEGER NOT NULL DEFAULT 0,
    quality_score           REAL NOT NULL,                -- nodes_ok / nodes_total
    orphaned_at             INTEGER,                       -- unix ts; NULL = not orphaned
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ir_cache_orphaned
    ON ir_cache_index(orphaned_at)
    WHERE orphaned_at IS NOT NULL;

-- Per-source IR provenance + repair counts. New columns only — extraction_method
-- and quality_score already exist (migration 016).
ALTER TABLE sources ADD COLUMN ir_cache_sha          TEXT;        -- FK → ir_cache_index.sha256
ALTER TABLE sources ADD COLUMN ir_quality_score      REAL;        -- snapshot at import time (distinct from the OCR quality_score)
ALTER TABLE sources ADD COLUMN nodes_low_confidence  INTEGER;     -- snapshot at import time

CREATE INDEX IF NOT EXISTS idx_sources_ir_cache_sha ON sources(ir_cache_sha);

-- Repair audit log — append-only.
CREATE TABLE IF NOT EXISTS ir_repair_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       TEXT NOT NULL,                        -- FK → sources.id
    sha256          TEXT NOT NULL,                        -- FK → ir_cache_index.sha256
    node_id         TEXT NOT NULL,                        -- DocIr.nodes[].id
    action          TEXT NOT NULL,                        -- 'edit_text'|'edit_kind'|'add_sibling'|'delete'|'edit_annotation'
    before_text     TEXT,                                 -- nullable; for add_sibling
    after_text      TEXT,                                 -- nullable; for delete
    repaired_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ir_repair_source ON ir_repair_log(source_id);
CREATE INDEX IF NOT EXISTS idx_ir_repair_sha    ON ir_repair_log(sha256);
