-- Feature 009 (Conversational session, Phase A) — durable chat transcript per
-- draft. Additive only; does NOT bump TARGET_SCHEMA_VERSION (existing indexed
-- corpora preserved, no wipe). Every officer/system turn is stored (FR-031) and
-- the full transcript is restorable on resume (FR-032). `conversation_summary`
-- is created now but stays unused until Phase C (rolling-summary compaction,
-- FR-033/034) — no compaction logic ships in Phase A.

CREATE TABLE IF NOT EXISTS chat_message (
    id                TEXT PRIMARY KEY,
    draft_session_id  TEXT NOT NULL,
    seq               INTEGER NOT NULL,
    role              TEXT NOT NULL,          -- 'officer' | 'system'
    content           TEXT NOT NULL,
    token_count       INTEGER,
    summarized        INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    UNIQUE (draft_session_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_chat_message_session ON chat_message(draft_session_id, seq);

CREATE TABLE IF NOT EXISTS conversation_summary (
    draft_session_id  TEXT PRIMARY KEY,
    summary_text      TEXT NOT NULL,
    covers_upto_seq   INTEGER NOT NULL,
    updated_at        TEXT NOT NULL
);
