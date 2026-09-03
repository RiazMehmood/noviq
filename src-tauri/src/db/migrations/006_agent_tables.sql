-- ============================================================
-- EchoAI — Agent task tables (migration 004)
-- ============================================================

-- ─── agent_tasks ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS agent_tasks (
    id             TEXT PRIMARY KEY,                    -- UUID v4
    instruction    TEXT NOT NULL,
    input_mode     TEXT NOT NULL DEFAULT 'chat'
                   CHECK (input_mode IN ('chat','voice')),
    status         TEXT NOT NULL DEFAULT 'queued'
                   CHECK (status IN (
                       'queued','planning','awaiting_approval',
                       'running','paused','completed','failed','cancelled'
                   )),
    queue_position INTEGER,
    result_text    TEXT,
    error_text     TEXT,
    created_at     INTEGER NOT NULL,                    -- Unix ms
    updated_at     INTEGER NOT NULL
);

-- ─── agent_tasks_fts ──────────────────────────────────────────────────────────
CREATE VIRTUAL TABLE IF NOT EXISTS agent_tasks_fts USING fts5(
    instruction,
    content     = 'agent_tasks',
    content_rowid = 'rowid'
);

CREATE TRIGGER IF NOT EXISTS agent_tasks_ai AFTER INSERT ON agent_tasks BEGIN
    INSERT INTO agent_tasks_fts(rowid, instruction) VALUES (new.rowid, new.instruction);
END;

CREATE TRIGGER IF NOT EXISTS agent_tasks_au AFTER UPDATE ON agent_tasks BEGIN
    INSERT INTO agent_tasks_fts(agent_tasks_fts, rowid, instruction) VALUES ('delete', old.rowid, old.instruction);
    INSERT INTO agent_tasks_fts(rowid, instruction) VALUES (new.rowid, new.instruction);
END;

CREATE TRIGGER IF NOT EXISTS agent_tasks_ad AFTER DELETE ON agent_tasks BEGIN
    INSERT INTO agent_tasks_fts(agent_tasks_fts, rowid, instruction) VALUES ('delete', old.rowid, old.instruction);
END;

-- ─── agent_steps ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS agent_steps (
    id              TEXT PRIMARY KEY,                   -- UUID v4
    task_id         TEXT NOT NULL REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_number     INTEGER NOT NULL,
    tool_name       TEXT,
    input_json      TEXT,
    output_text     TEXT,
    generated_code  TEXT,
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending','running','completed','failed','skipped')),
    reversibility   TEXT NOT NULL DEFAULT 'reversible'
                    CHECK (reversibility IN ('reversible','irreversible')),
    duration_ms     INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- ─── agent_steps_fts ──────────────────────────────────────────────────────────
CREATE VIRTUAL TABLE IF NOT EXISTS agent_steps_fts USING fts5(
    output_text,
    content     = 'agent_steps',
    content_rowid = 'rowid'
);

CREATE TRIGGER IF NOT EXISTS agent_steps_ai AFTER INSERT ON agent_steps BEGIN
    INSERT INTO agent_steps_fts(rowid, output_text) VALUES (new.rowid, new.output_text);
END;

CREATE TRIGGER IF NOT EXISTS agent_steps_au AFTER UPDATE ON agent_steps BEGIN
    INSERT INTO agent_steps_fts(agent_steps_fts, rowid, output_text) VALUES ('delete', old.rowid, old.output_text);
    INSERT INTO agent_steps_fts(rowid, output_text) VALUES (new.rowid, new.output_text);
END;

CREATE TRIGGER IF NOT EXISTS agent_steps_ad AFTER DELETE ON agent_steps BEGIN
    INSERT INTO agent_steps_fts(agent_steps_fts, rowid, output_text) VALUES ('delete', old.rowid, old.output_text);
END;
