-- 014 — deep-search checkpointing (Feature 026, Part 1).
--
-- LangGraph-style durable state for the bounded agentic deep-search loop
-- (expand → retrieve_r1 → coverage → retrieve_r2 → synthesize). One row is
-- written after each node completes, so a turn can RESUME after a restart /
-- reconnect (continue from the next node instead of re-embedding done rounds)
-- and TIME-TRAVEL (re-run forward from an earlier checkpoint via `parent_id`,
-- which forms the branch DAG). `state_json` carries the serialized `DeepState`
-- (sub-questions, passage set, coverage, partial answer, trace counters) —
-- mirrors the `one_ask_session` state_json convention (ADR-0014).
--
-- Additive: a brand-new table, nothing else touched.

CREATE TABLE IF NOT EXISTS deep_checkpoint (
    id          TEXT PRIMARY KEY,        -- checkpoint uuid
    thread_id   TEXT NOT NULL,           -- conversation_id + ':' + turn_seq (one deep turn)
    seq         BIGINT NOT NULL,         -- super-step index within the thread (0..N)
    node        TEXT NOT NULL,           -- last completed node: expand|retrieve_r1|coverage|retrieve_r2|synthesize|done
    parent_id   TEXT,                    -- previous checkpoint → the time-travel branch DAG
    state_json  TEXT NOT NULL,           -- serialized DeepState
    status      TEXT NOT NULL,           -- running|interrupted|done|error
    created_at  BIGINT NOT NULL          -- ms since epoch
);

-- Resume/time-travel read path: latest / ordered checkpoints for one thread.
CREATE INDEX IF NOT EXISTS ix_deep_ckpt_thread ON deep_checkpoint (thread_id, seq);
