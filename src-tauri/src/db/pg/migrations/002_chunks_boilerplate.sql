-- US8/R2 (FR-013b) — advisory boilerplate flag.
--
-- Retrieval excludes flagged chunks from ranking WITHOUT deleting them
-- (flag-not-delete: reversible + auditable; dropping a real provision would be
-- silent and unrecoverable). Set at persist time by the generic, structural
-- classifier in services/parsers/boilerplate.rs, guarded by the
-- provision-label-keep invariant. Additive; default FALSE so existing rows are
-- unaffected until a re-index.
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS is_boilerplate BOOLEAN NOT NULL DEFAULT FALSE;
