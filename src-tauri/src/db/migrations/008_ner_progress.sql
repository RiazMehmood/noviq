-- Migration 008: NER extraction progress tracking per source.
-- Allows the KG tab to show a progress bar while Pass 2 LLM extraction runs,
-- and lock the graph until all sources in the notebook are fully extracted.

ALTER TABLE sources ADD COLUMN ner_status   TEXT    NOT NULL DEFAULT 'done'
    CHECK (ner_status IN ('pending','running','done'));
ALTER TABLE sources ADD COLUMN ner_progress INTEGER NOT NULL DEFAULT 100;
ALTER TABLE sources ADD COLUMN ner_total    INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_sources_ner_status ON sources(ner_status);
