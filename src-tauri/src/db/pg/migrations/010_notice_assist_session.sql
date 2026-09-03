-- Feature 022 (Notice Assist): per-conversation slot-filling session for the
-- `/help` officer-defense flow. One session per conversation; the whole
-- aggregate lives in `state_json` (mirrors one_ask_session, ADR-0014).
-- Additive only; idempotent.
CREATE TABLE IF NOT EXISTS notice_assist_session (
    conversation_id TEXT PRIMARY KEY,
    state_json      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
