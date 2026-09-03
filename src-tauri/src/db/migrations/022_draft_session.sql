-- Feature 009 (US5) — persistent, revisable draft sessions.
-- Additive only; does NOT bump TARGET_SCHEMA_VERSION, so existing indexed
-- corpora are preserved (no wipe). A draft is a living working state (FR-023):
-- the officer refines across turns until finalize, and an unfinished draft can be
-- resumed after restart. The whole working state is stored as one JSON blob so
-- the in-memory `DraftState` serialises 1:1 (template id, intent, candidate +
-- confirmed docs, current rendered blocks, per-prose edits, gap resolutions).
CREATE TABLE IF NOT EXISTS draft_session (
    draft_id    TEXT PRIMARY KEY,
    state_json  TEXT NOT NULL,
    finalized   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_draft_session_finalized ON draft_session(finalized);
