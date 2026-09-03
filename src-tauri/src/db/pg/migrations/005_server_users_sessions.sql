-- F014 (ADR-0017) — server / thin-client foundation.
-- ADDITIVE ONLY: new tables + nullable columns. Does NOT bump
-- TARGET_SCHEMA_VERSION's contract in a breaking way; an older binary remains
-- valid against this schema (the new columns are nullable / unread by it), so
-- a failed deploy can roll back safely (FR-017).

-- ── Users (pilot: a flat set of 2–3; RBAC columns reserved, unenforced) ──────
CREATE TABLE IF NOT EXISTS users (
    id            TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,            -- argon2id; never returned to clients
    created_at    BIGINT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'user',  -- 'admin' | 'user' (admin = beta user mgmt, US7)
    disabled_at   BIGINT,                    -- non-NULL = account disabled (US7); login rejected
    department_id TEXT                        -- reserved for RBAC follow-on (FR-010)
);

-- ── Sessions (opaque bearer token, stored hashed; expiry/logout revocation) ──
CREATE TABLE IF NOT EXISTS session (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL,             -- hash of the bearer token; raw never stored
    created_at   BIGINT NOT NULL,
    expires_at   BIGINT NOT NULL,
    last_seen_at BIGINT
);
CREATE INDEX IF NOT EXISTS idx_session_token   ON session(token_hash);
CREATE INDEX IF NOT EXISTS idx_session_user    ON session(user_id);
CREATE INDEX IF NOT EXISTS idx_session_expires ON session(expires_at);

-- ── sources: reserved RBAC columns (nullable, NOT enforced this feature) ─────
ALTER TABLE sources ADD COLUMN IF NOT EXISTS department_id TEXT;
ALTER TABLE sources ADD COLUMN IF NOT EXISTS owner_user_id TEXT;
ALTER TABLE sources ADD COLUMN IF NOT EXISTS visibility    TEXT;  -- 'private' | 'department'; NULL = current shared behavior

-- ── audit_log: multi-user traceability (FR-011) ─────────────────────────────
ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS user_id TEXT;
ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS action  TEXT;
ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS target  TEXT;
