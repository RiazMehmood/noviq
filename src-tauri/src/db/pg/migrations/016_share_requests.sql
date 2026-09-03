-- 2026-07-28 — admin-gated sharing.
-- A non-admin owner can no longer flip a document to 'shared' directly; the
-- share button files a request the admin reviews in the Admin console.
-- One pending request per source (partial unique index); decided rows are
-- kept as history. Additive only.
--
-- NOTE deliberately NOT changed: sources.visibility DEFAULT stays 'shared' —
-- several indexer insert paths (file-watcher / corpus folder indexing) omit
-- the column and rely on that default to publish corpus documents; flipping
-- it would silently privatize admin-indexed corpus files.
CREATE TABLE IF NOT EXISTS share_requests (
    id                 TEXT   PRIMARY KEY,
    source_id          TEXT   NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    requester_user_id  TEXT   NOT NULL,
    status             TEXT   NOT NULL DEFAULT 'pending',  -- pending | approved | rejected
    created_at         BIGINT NOT NULL,
    decided_at         BIGINT,
    decided_by         TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS share_requests_one_pending
    ON share_requests(source_id) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS share_requests_status ON share_requests(status);
