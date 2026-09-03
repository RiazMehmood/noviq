-- Idea 2 (2026-05-23) — incremental Pass-2 NER coverage.
--
-- Pass-2 entity extraction (LLM-based, slow) now processes the corpus in
-- bounded batches (~10 % of total chunks per batch) over multiple passes
-- until 100 % coverage is reached. We need per-chunk progress so the
-- "next batch" selector knows which chunks are still pending.
--
-- All existing chunks default to 0 (pending) — on the first app launch
-- after this migration, the iterative loop will process them in batches,
-- gated by the chat-priority scheduler. Already-processed chunks (from
-- previous fixed-cap runs) will be re-extracted; that is idempotent
-- because `entity_extractor::extract_pass2` upserts.
ALTER TABLE chunks
    ADD COLUMN ner_pass2_done INTEGER NOT NULL DEFAULT 0
    CHECK (ner_pass2_done IN (0, 1));

-- Index lets the batch selector cheaply find pending chunks per source.
CREATE INDEX IF NOT EXISTS idx_chunks_ner_pass2_pending
    ON chunks(source_id, ner_pass2_done)
    WHERE ner_pass2_done = 0;
