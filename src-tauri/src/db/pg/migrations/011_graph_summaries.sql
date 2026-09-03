-- 011 — Track 2 (ADR-0019 §2): per-provision + per-document plain-language
-- SUMMARIES used ONLY as a recall/navigation index. The answer path never reads
-- `summary_text` — a summary row maps back to its ORIGINAL provision chunk
-- (`chunk_id`), and only that original text is ever shown/cited (verbatim
-- contract). Additive; no existing table touched.
--
-- The embedding lives in a SEPARATE runtime table `summary_embeddings` (created
-- by schema.rs::ensure_summary_vector_table) because the vector dimension is
-- resolved at runtime from db_config (768/384) — exactly like `chunk_embeddings`.

CREATE TABLE IF NOT EXISTS graph_summary (
    id           TEXT PRIMARY KEY,
    source_id    TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    -- The ORIGINAL provision chunk this summary points back to. The answer pulls
    -- THIS chunk's verbatim text; the summary is only the recall key.
    chunk_id     TEXT NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
    citation     TEXT,
    unit_path    TEXT,
    level        TEXT NOT NULL CHECK (level IN ('provision','document')),
    summary_text TEXT NOT NULL,
    created_at   BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_graph_summary_source ON graph_summary(source_id);
CREATE INDEX IF NOT EXISTS idx_graph_summary_chunk  ON graph_summary(chunk_id);
