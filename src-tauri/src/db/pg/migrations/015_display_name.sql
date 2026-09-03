-- 2026-07-28 — display titles for citations.
-- Human-readable document names derived from file names (extension stripped,
-- CamelCase/underscores split into words). Populated by Rust at index time,
-- plus a one-time startup backfill for existing rows (db/schema.rs).
-- Additive and nullable; readers fall back to file_name when NULL.
ALTER TABLE sources ADD COLUMN IF NOT EXISTS display_name TEXT;
