-- Feature 009 — LGD Summary & Note Drafting.
-- Additive only (no column/table drops); does NOT bump TARGET_SCHEMA_VERSION,
-- so existing indexed corpora are preserved (no wipe). Stores reusable template
-- slot-schemas and gold/style example references. Gold example CONTENT lives on
-- disk and is NEVER inserted into the retrieval corpus (chunks/FTS/vec) — SC-007.

-- A reusable structure derived from an uploaded format.
CREATE TABLE IF NOT EXISTS template_schema (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    name             TEXT NOT NULL,
    instrument_type  TEXT NOT NULL CHECK (instrument_type IN ('summary','note')),
    source_format_ref TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    UNIQUE (instrument_type, name)
);

-- An ordered region of a template: boilerplate | cited | prose.
CREATE TABLE IF NOT EXISTS template_slot (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    template_id              INTEGER NOT NULL REFERENCES template_schema(id) ON DELETE CASCADE,
    ordering                 INTEGER NOT NULL,
    kind                     TEXT NOT NULL CHECK (kind IN ('boilerplate','cited','prose')),
    label                    TEXT,
    boilerplate_text         TEXT,
    required_source_category TEXT,
    prose_guidance           TEXT,
    UNIQUE (template_id, ordering)
);

CREATE INDEX IF NOT EXISTS idx_template_slot_template ON template_slot(template_id);

-- An uploaded filled instrument kept for structure/style + evaluation.
-- content_ref points at on-disk content; excluded from retrieval (SC-007).
CREATE TABLE IF NOT EXISTS gold_example (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    template_id      INTEGER REFERENCES template_schema(id) ON DELETE SET NULL,
    instrument_type  TEXT NOT NULL CHECK (instrument_type IN ('summary','note')),
    content_ref      TEXT NOT NULL,
    created_at       TEXT NOT NULL
);
