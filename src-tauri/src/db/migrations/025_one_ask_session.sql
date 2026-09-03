-- ============================================================
-- Migration 025 — Feature 010 one-ask drafting session (additive)
-- Research R7 / ADR-0014: parallel table to 009 draft_session so the
-- two serde state shapes never mix; ANSI-portable column types
-- (TEXT uuid ids, TEXT ISO-8601 timestamps → uuid/timestamptz later).
-- ============================================================

CREATE TABLE IF NOT EXISTS one_ask_session (
    draft_id    TEXT PRIMARY KEY,
    state_json  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
