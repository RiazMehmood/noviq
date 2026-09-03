-- Migration 009: Replace narrow entity types with a universal schema.
-- Previous types (email, project, decision) were too domain-specific —
-- they matched almost nothing in legal, medical, engineering, or economics documents.
-- New types: person, org, location, concept, term, reference, date, amount.
--
-- Data migration:
--   email     → org  (email addresses usually identify organisations)
--   project   → concept
--   decision  → concept

PRAGMA foreign_keys = OFF;

CREATE TABLE entities_new (
    id               TEXT PRIMARY KEY,
    canonical_value  TEXT NOT NULL,
    entity_type      TEXT NOT NULL
                     CHECK (entity_type IN ('person','org','location','concept','term','reference','date','amount')),
    normalized_value TEXT,
    source_count     INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL,
    UNIQUE(canonical_value, entity_type)
);

-- INSERT OR IGNORE handles the case where email→org collides with an existing org row.
INSERT OR IGNORE INTO entities_new (id, canonical_value, entity_type, normalized_value, source_count, created_at)
SELECT
    id,
    canonical_value,
    CASE entity_type
        WHEN 'email'    THEN 'org'
        WHEN 'project'  THEN 'concept'
        WHEN 'decision' THEN 'concept'
        ELSE entity_type
    END,
    normalized_value,
    source_count,
    created_at
FROM entities;

DROP TABLE entities;
ALTER TABLE entities_new RENAME TO entities;

CREATE INDEX IF NOT EXISTS idx_entities_type  ON entities(entity_type);
CREATE INDEX IF NOT EXISTS idx_entities_value ON entities(canonical_value);

PRAGMA foreign_keys = ON;
