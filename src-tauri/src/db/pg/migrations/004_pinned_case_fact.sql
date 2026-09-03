-- Feature 015 — Unified Bureaucrat Chat Assistant. Additive only (no drops).
-- T002 / data-model.md.

-- Load-bearing situational facts the user stated (FR-018): kept verbatim in the
-- allocator's always-present pinned slot; never evicted, never summarized;
-- supersedable. id/conversation_id are TEXT to match the existing TEXT ids
-- (chat_message.draft_session_id is TEXT).
CREATE TABLE IF NOT EXISTS pinned_case_fact (
    id              TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    conversation_id TEXT NOT NULL,
    key             TEXT NOT NULL,
    value           TEXT NOT NULL,
    source_seq      INTEGER,
    superseded      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_pinned_fact_conv
    ON pinned_case_fact (conversation_id) WHERE superseded = FALSE;

-- Additive conversation identity on chat_message (resolves analyze-I1).
-- All F015 tables key on conversation_id; existing rows backfill from the
-- draft_session_id so legacy drafting threads keep working.
ALTER TABLE chat_message ADD COLUMN IF NOT EXISTS conversation_id TEXT;
UPDATE chat_message SET conversation_id = draft_session_id WHERE conversation_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_chat_message_conv ON chat_message (conversation_id, seq);
