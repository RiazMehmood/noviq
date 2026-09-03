-- 2026-07-29 — per-user preferences.
--
-- The only user-facing toggle so far (`chat_extractive`) lives in `db_config`
-- and is deliberately SERVER-WIDE. That is the wrong shape for preferences an
-- individual officer sets for themselves, so this table is keyed by user.
--
-- First consumer: `refinement_offer` — whether an answer ends with a "Next
-- steps" follow-up offer. Absent row = enabled, so existing users keep today's
-- behaviour and the feature needs no backfill.
--
-- Generic key/value on purpose: the next preference should not need a
-- migration. Values are TEXT ('1'/'0' for booleans) for the same reason.
-- Additive only.
CREATE TABLE IF NOT EXISTS user_preference (
    user_id    TEXT   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key        TEXT   NOT NULL,
    value      TEXT   NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, key)
);
