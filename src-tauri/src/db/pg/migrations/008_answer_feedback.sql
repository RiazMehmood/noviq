-- Feature 018 — Answer Feedback Loop (Beta).
-- ADDITIVE ONLY: one new table + indexes. An older binary remains valid against
-- this schema (a failed deploy can roll back). No existing table is touched.
--
-- Officers rate every answer with TWO single-select citation questions:
--   citation_accuracy     — all | some | wrong | unsure
--   citation_completeness — yes  | missing | na
-- Each response is anchored to the exact assistant turn = chat_message.id
-- (returned to the client as TurnResponse.message_id). Either question may be
-- answered independently; a re-tap SUPERSEDES the prior row (history retained,
-- one CURRENT row per (message_id, user)).

CREATE TABLE IF NOT EXISTS answer_feedback (
    id                     TEXT PRIMARY KEY,
    message_id             TEXT NOT NULL,          -- rated assistant turn = chat_message.id
    conversation_id        TEXT,                   -- denormalized for admin JOIN to query_log
    user_id                TEXT,                   -- web = session user; NULL for desktop
    citation_accuracy      TEXT,                   -- all | some | wrong | unsure  (NULL until answered)
    citation_completeness  TEXT,                   -- yes | missing | na           (NULL until answered)
    supersedes             TEXT,                   -- answer_feedback.id this row replaces (NULL for first)
    superseded_at          BIGINT,                 -- epoch ms; NULL = CURRENT row
    created_at             BIGINT NOT NULL,        -- epoch ms

    CONSTRAINT ck_answer_feedback_accuracy
        CHECK (citation_accuracy IS NULL
               OR citation_accuracy IN ('all','some','wrong','unsure')),
    CONSTRAINT ck_answer_feedback_completeness
        CHECK (citation_completeness IS NULL
               OR citation_completeness IN ('yes','missing','na')),
    CONSTRAINT ck_answer_feedback_nonempty
        CHECK (citation_accuracy IS NOT NULL OR citation_completeness IS NOT NULL)
);

-- Exactly one CURRENT row per (message_id, user). COALESCE folds the desktop NULL
-- user to '' so PG's "NULLs are distinct" rule doesn't allow duplicate currents.
CREATE UNIQUE INDEX IF NOT EXISTS uq_answer_feedback_current
    ON answer_feedback (message_id, COALESCE(user_id, ''))
    WHERE superseded_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_answer_feedback_message ON answer_feedback (message_id);
CREATE INDEX IF NOT EXISTS idx_answer_feedback_created ON answer_feedback (created_at);
