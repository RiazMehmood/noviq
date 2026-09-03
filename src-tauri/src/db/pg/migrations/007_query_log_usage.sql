-- F014 follow-on — force LLM chat + per-user query/usage logging.
-- ADDITIVE ONLY: resets one config row + adds one table. An older binary
-- remains valid against this schema, so a failed deploy can roll back (FR-017).

-- ── Force LLM-only chat ──────────────────────────────────────────────────────
-- The extractive bypass defaulted ON and had no HTTP/admin toggle, so the live
-- server answered without the model. Reset the stored flag to '0' so chat uses
-- the LLM. (Re-enable later only by setting this key to '1'.)
INSERT INTO db_config (key, value) VALUES ('chat_extractive', '0')
  ON CONFLICT (key) DO UPDATE SET value = '0';

-- ── Per-user query + resource log (admin usage visibility) ───────────────────
-- One row per chat turn: who asked, what they asked, the answer, citations, how
-- long it took, and the peak process/GPU resource use sampled during the turn
-- (generation is serialized, so the sample attributes cleanly to one user).
CREATE TABLE IF NOT EXISTS query_log (
    id              TEXT PRIMARY KEY,
    user_id         TEXT,                 -- NULL only for the desktop (no auth)
    conversation_id TEXT,
    question        TEXT NOT NULL,
    answer          TEXT,                 -- final answer body (may be empty)
    citations_json  TEXT,                 -- serialized citation list
    latency_ms      BIGINT,               -- wall-clock for the turn
    cpu_pct         REAL,                 -- peak server-process CPU% during turn
    ram_mb          REAL,                 -- peak server-process RSS (MB)
    gpu_util_pct    REAL,                 -- peak GPU utilization% (NULL if no GPU)
    gpu_mem_mb      REAL,                 -- peak GPU memory used MB (NULL if none)
    created_at      BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_query_log_user ON query_log(user_id);
CREATE INDEX IF NOT EXISTS idx_query_log_time ON query_log(created_at);
