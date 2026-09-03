-- ============================================================
-- EchoAI — Agent support tables (migration 005)
-- ============================================================

-- ─── pending_actions (PAQ — Plan-and-Approve Queue) ──────────────────────────
CREATE TABLE IF NOT EXISTS pending_actions (
    id           TEXT PRIMARY KEY,                      -- UUID v4
    task_id      TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_id      TEXT REFERENCES agent_steps(id) ON DELETE CASCADE,
    description  TEXT NOT NULL,
    safety_class TEXT NOT NULL DEFAULT 'CAUTIOUS'
                 CHECK (safety_class IN ('READ_ONLY','CAUTIOUS','DANGEROUS')),
    status       TEXT NOT NULL DEFAULT 'awaiting'
                 CHECK (status IN ('awaiting','approved','rejected')),
    decided_at   INTEGER,
    created_at   INTEGER NOT NULL
);

-- ─── execution_log ───────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS execution_log (
    id          TEXT PRIMARY KEY,                       -- UUID v4
    task_id     TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_id     TEXT REFERENCES agent_steps(id) ON DELETE CASCADE,
    event_type  TEXT NOT NULL,                          -- status transition name
    detail_json TEXT,
    created_at  INTEGER NOT NULL
);

-- ─── rollback_log ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS rollback_log (
    id           TEXT PRIMARY KEY,                      -- UUID v4
    task_id      TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_id      TEXT NOT NULL REFERENCES agent_steps(id) ON DELETE CASCADE,
    operation    TEXT NOT NULL,                         -- e.g. "file_write", "file_move"
    params_json  TEXT NOT NULL,                         -- original params for undo
    undo_params  TEXT NOT NULL,                         -- params needed to reverse
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending','completed','failed','skipped')),
    created_at   INTEGER NOT NULL
);

-- ─── agent_personas ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS agent_personas (
    id          TEXT PRIMARY KEY,                       -- persona id from TOML
    version     TEXT NOT NULL,
    toml_path   TEXT NOT NULL,
    config_json TEXT NOT NULL,                          -- serialised PersonaConfig
    loaded_at   INTEGER NOT NULL
);

-- ─── agent_settings ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS agent_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Defaults
INSERT OR IGNORE INTO agent_settings (key, value) VALUES
    ('wake_phrase',          'EchoAI'),
    ('voice_mode',           'chat_only'),
    ('tts_enabled',          'false'),
    ('history_limit',        '500'),
    ('allowed_paths',        '[]'),
    ('overlay_start_visible','false');
