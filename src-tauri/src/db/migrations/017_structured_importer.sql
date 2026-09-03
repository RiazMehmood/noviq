-- Feature 005 — generic structured importer.
-- Adds per-source columns for the resolved profile + structure pipeline outcome
-- + version tag used by the startup re-index hook (mirrors Feature 004's
-- extraction_method / quality_score pattern).

ALTER TABLE sources ADD COLUMN doc_type         TEXT;
ALTER TABLE sources ADD COLUMN doc_type_source  TEXT
                     CHECK (doc_type_source IN ('auto','llm','user'));
ALTER TABLE sources ADD COLUMN structure_status TEXT
                     CHECK (structure_status IN ('structured','failed','skipped'));
ALTER TABLE sources ADD COLUMN structure_importer_version INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_sources_structure_version ON sources(structure_importer_version);
