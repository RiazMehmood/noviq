-- Migration 024 (F027) — mark a source that was UPLOADED INTO a project.
--
-- Reported 2026-08-11: "still it is showing my projects documents in chat
-- window". A document uploaded into a project was indexed like any other
-- personal document, so it appeared in the officer's global corpus tree and in
-- global retrieval. A project exists precisely to be a separate working set.
--
-- ⭐ This mirrors `sources.conversation_id`, which has marked conversation
-- attachments since F021 and is exactly the same shape of fact: "this document
-- belongs to one place and must not surface in the general listings". Following
-- that precedent rather than inventing a path convention (`__project__…`) keeps
-- one mechanism for one idea — a path is a heuristic, a column is a fact.
--
-- ⚠️ NOT the same as `project_source`, and both are needed:
--   * `project_source` = MEMBERSHIP. Many-to-many, and the basis of retrieval
--     scoping. A corpus document ADDED to a project gets a row here and stays
--     globally visible, because adding is a reference, not a move (ADR-0028 —
--     sharing is a copy; the same principle).
--   * `sources.project_id` = ORIGIN. At most one, set only by the upload path.
--     This is what hides the document from the global tree.
-- A document can therefore be in three projects and originate in none.
--
-- ON DELETE SET NULL: deleting a project must not destroy a document (FR-024
-- guarantees the opposite). The document simply stops being project-only and
-- returns to the officer's general listing, which is the safe direction.
ALTER TABLE sources
    ADD COLUMN IF NOT EXISTS project_id TEXT
    REFERENCES project(id) ON DELETE SET NULL;

-- The listings and the retrieval scope both filter on `project_id IS NULL`, so
-- this index serves the common case (the vast majority of rows are NULL and
-- Postgres can skip them wholesale).
CREATE INDEX IF NOT EXISTS idx_sources_project_id
    ON sources (project_id)
    WHERE project_id IS NOT NULL;
