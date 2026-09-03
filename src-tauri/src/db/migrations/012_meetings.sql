-- 012_meetings.sql — Meeting Agent Phase 2 (additive)

CREATE TABLE meetings (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    title        TEXT    NOT NULL,
    started_at   TEXT    NOT NULL,
    ended_at     TEXT,
    language     TEXT    NOT NULL,
    model_path   TEXT    NOT NULL,
    status       TEXT    NOT NULL DEFAULT 'active'
                 CHECK (status IN ('active','stopped','error')),
    error        TEXT
);

CREATE TABLE meeting_segments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id   INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    t0_ms        INTEGER NOT NULL,
    t1_ms        INTEGER NOT NULL,
    text         TEXT    NOT NULL,
    created_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_meeting_segments_meeting_id
    ON meeting_segments(meeting_id, t0_ms);
