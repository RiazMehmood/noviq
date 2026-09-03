-- 012 — presentational WHOLE-DOCUMENT summary (user-facing).
--
-- Distinct from the 011 recall-index rows. A `level='document_full'` row holds a
-- complete plain-language summary of an entire document, generated ONCE at ingest
-- (reduced over the per-provision gists — NOT a live map-reduce). It is returned
-- verbatim when the user asks to "summarize <named document>": deterministic, no
-- model call at query time, and — by product decision — carries NO citations.
--
-- Long documents are split into a few ordered segments (one row each, sequence in
-- `unit_path`); short documents get a single row. These rows are intentionally
-- NOT embedded into `summary_embeddings`, so they never enter the recall pool.
--
-- Additive: widen the level CHECK to admit the new value.
ALTER TABLE graph_summary DROP CONSTRAINT IF EXISTS graph_summary_level_check;
ALTER TABLE graph_summary ADD CONSTRAINT graph_summary_level_check
    CHECK (level IN ('provision', 'document', 'document_full'));
