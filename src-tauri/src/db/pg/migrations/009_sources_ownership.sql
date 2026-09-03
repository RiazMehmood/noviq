-- Feature 021 (user documents): ownership + visibility scope on sources.
-- `sources` is the retrieval-scope truth (research D1): every retrieval query
-- already joins it, so one indexed predicate enforces privacy everywhere.
-- Additive only; chunks/embeddings untouched (FR-015 — no re-index).
--
-- GOTCHA (2026-07-05 crashloop): migration 005 ALREADY added a nullable,
-- unused `sources.visibility TEXT` ('private'|'department', NULL = shared).
-- A plain ADD COLUMN visibility therefore fails and takes the server down at
-- boot. This file adopts + normalizes the existing column instead, and every
-- statement is idempotent (IF NOT EXISTS) so a partially-applied run can
-- always complete on retry.

ALTER TABLE sources ADD COLUMN IF NOT EXISTS owner_id TEXT;
ALTER TABLE sources ADD COLUMN IF NOT EXISTS conversation_id TEXT;

-- Adopt the pre-existing visibility column (005): NULL meant "current shared
-- behaviour"; 'department' was never written. Normalize everything to the
-- F021 two-value model, then enforce it going forward. (No CHECK constraint:
-- ADD CONSTRAINT has no IF NOT EXISTS form; the two-value rule is enforced by
-- the API layer and by NOT NULL + DEFAULT here.)
UPDATE sources SET visibility = 'shared'
 WHERE visibility IS NULL OR visibility NOT IN ('private', 'shared');
ALTER TABLE sources ALTER COLUMN visibility SET DEFAULT 'shared';
ALTER TABLE sources ALTER COLUMN visibility SET NOT NULL;

ALTER TABLE indexed_folders
    ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'shared';

CREATE INDEX IF NOT EXISTS idx_sources_owner ON sources (owner_id);
CREATE INDEX IF NOT EXISTS idx_sources_conversation ON sources (conversation_id);
-- Partial index covering the global-corpus arm of visible_source_ids().
CREATE INDEX IF NOT EXISTS idx_sources_shared ON sources (id)
    WHERE visibility = 'shared' AND conversation_id IS NULL;

-- ── Backfill ────────────────────────────────────────────────────────────────
-- Sources inherit owner/visibility from their owning folder through the
-- stub-notebook link (`__folder__<folder id>`). Admin/global corpus
-- (owner_id NULL) stays 'shared'; user-owned folders were private-in-intent
-- (F014 upload flow), so their sources become private.

UPDATE sources s
SET    owner_id = f.owner_id
FROM   notebook_sources ns
JOIN   indexed_folders f ON ns.notebook_id = '__folder__' || f.id
WHERE  ns.source_id = s.id
  AND  f.owner_id IS NOT NULL;

UPDATE sources s
SET    visibility = 'private'
FROM   notebook_sources ns
JOIN   indexed_folders f ON ns.notebook_id = '__folder__' || f.id
WHERE  ns.source_id = s.id
  AND  f.owner_id IS NOT NULL;

UPDATE indexed_folders
SET    visibility = CASE WHEN owner_id IS NULL THEN 'shared' ELSE 'private' END;
