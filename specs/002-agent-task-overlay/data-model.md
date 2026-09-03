# Data Model: Agent Task Executor + Desktop Overlay

**Branch**: `002-agent-task-overlay` | **Date**: 2026-04-19

All tables are additive migrations to the shared `echo.db`. No existing App 1 tables are modified.

---

## Entity Relationship Overview

```
agent_tasks (1) ──── (N) agent_steps
agent_tasks (1) ──── (N) pending_actions
agent_tasks (1) ──── (N) execution_log
agent_tasks (1) ──── (N) rollback_log
agent_steps  (1) ──── (N) pending_actions  [step that triggered the approval]
agent_steps  (1) ──── (N) rollback_log     [undo record for this step]
agent_personas (1) ──── (N) agent_tasks    [persona used for task]

App 1 read-only references (no FK — soft reference by path/id):
agent_tasks → sources (via task context injection in ReAct prompt)
```

---

## Tables

### `agent_tasks`

The top-level record for a user-submitted instruction.

```sql
CREATE TABLE IF NOT EXISTS agent_tasks (
    id              TEXT    PRIMARY KEY,          -- UUID v4
    instruction     TEXT    NOT NULL,             -- user's natural language instruction
    input_mode      TEXT    NOT NULL              -- 'chat' | 'voice'
                    CHECK (input_mode IN ('chat', 'voice')),
    status          TEXT    NOT NULL              -- lifecycle state
                    CHECK (status IN (
                        'queued', 'planning', 'awaiting_approval',
                        'running', 'paused', 'completed',
                        'failed', 'cancelled', 'interrupted'
                    )),
    queue_position  INTEGER,                      -- NULL when not queued
    persona_id      TEXT    NOT NULL              -- FK to agent_personas.id
                    REFERENCES agent_personas(id),
    created_at      INTEGER NOT NULL,             -- unix epoch ms
    started_at      INTEGER,
    completed_at    INTEGER,
    result_summary  TEXT                          -- final output text shown in overlay
);

CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status);
CREATE INDEX IF NOT EXISTS idx_agent_tasks_created ON agent_tasks(created_at DESC);

-- FTS5 virtual table for keyword search across task instructions and outputs
CREATE VIRTUAL TABLE IF NOT EXISTS agent_tasks_fts USING fts5(
    task_id UNINDEXED,
    instruction,
    result_summary,
    content=agent_tasks,
    content_rowid=rowid
);
```

**State transitions**:
```
queued → planning → awaiting_approval → running ⇄ paused → completed
                                                          → failed
                                                          → cancelled
                                    → running → interrupted  (process killed)
```

---

### `agent_steps`

A single atomic tool call within a task.

```sql
CREATE TABLE IF NOT EXISTS agent_steps (
    id              TEXT    PRIMARY KEY,          -- UUID v4
    task_id         TEXT    NOT NULL
                    REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_index      INTEGER NOT NULL,             -- 0-based ordering within task
    step_name       TEXT    NOT NULL,             -- human-readable label e.g. "Scanning ~/Documents"
    tool            TEXT    NOT NULL,             -- 'file_read' | 'file_write' | 'file_delete' |
                                                  -- 'file_move' | 'shell_exec' | 'code_exec' |
                                                  -- 'web_fetch' | 'llm_reason'
    reversibility   TEXT    NOT NULL              -- PAQ / history tagging
                    CHECK (reversibility IN ('reversible', 'irreversible', 'read_only')),
    status          TEXT    NOT NULL
                    CHECK (status IN ('pending', 'running', 'done', 'failed', 'skipped')),
    input_json      TEXT,                         -- tool call arguments as JSON
    output_text     TEXT,                         -- stdout / result text
    error_text      TEXT,                         -- stderr / error message
    generated_code  TEXT,                         -- code written by agent (code_exec only)
    started_at      INTEGER,
    completed_at    INTEGER,
    duration_ms     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_agent_steps_task ON agent_steps(task_id, step_index);

-- FTS5 for searching step output text
CREATE VIRTUAL TABLE IF NOT EXISTS agent_steps_fts USING fts5(
    step_id UNINDEXED,
    task_id UNINDEXED,
    output_text,
    content=agent_steps,
    content_rowid=rowid
);
```

---

### `pending_actions`

The Pending Actions Queue (PAQ) — constitution Principle V.

```sql
CREATE TABLE IF NOT EXISTS pending_actions (
    id              TEXT    PRIMARY KEY,          -- UUID v4
    task_id         TEXT    NOT NULL
                    REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_id         TEXT    NOT NULL
                    REFERENCES agent_steps(id) ON DELETE CASCADE,
    action_class    TEXT    NOT NULL              -- maps to tool safety_class in TOML
                    CHECK (action_class IN ('DANGEROUS', 'CAUTIOUS')),
    description     TEXT    NOT NULL,             -- human-readable: "Delete file ~/Downloads/old.txt"
    tool_call_json  TEXT    NOT NULL,             -- exact tool call args awaiting approval
    status          TEXT    NOT NULL
                    CHECK (status IN ('awaiting', 'approved', 'rejected')),
    created_at      INTEGER NOT NULL,
    decided_at      INTEGER,
    decided_by      TEXT                          -- 'user' | 'timeout_reject'
);

CREATE INDEX IF NOT EXISTS idx_pending_actions_task ON pending_actions(task_id, status);
```

---

### `execution_log`

Immutable audit trail of all agent events (append-only).

```sql
CREATE TABLE IF NOT EXISTS execution_log (
    id              TEXT    PRIMARY KEY,          -- UUID v4
    task_id         TEXT    NOT NULL
                    REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_id         TEXT                          -- NULL for task-level events
                    REFERENCES agent_steps(id),
    event_type      TEXT    NOT NULL
                    CHECK (event_type IN (
                        'task_created', 'task_planning_started',
                        'plan_shown', 'plan_approved', 'plan_rejected',
                        'step_started', 'step_completed', 'step_failed', 'step_skipped',
                        'approval_requested', 'approval_granted', 'approval_rejected',
                        'task_paused', 'task_resumed',
                        'task_completed', 'task_failed',
                        'task_cancelled', 'task_interrupted',
                        'rollback_started', 'rollback_step_done', 'rollback_completed'
                    )),
    payload_json    TEXT,                         -- event-specific data
    created_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_execution_log_task ON execution_log(task_id, created_at);
```

---

### `rollback_log`

Undo record for reversible file operations — enables Cancel recovery.

```sql
CREATE TABLE IF NOT EXISTS rollback_log (
    id              TEXT    PRIMARY KEY,          -- UUID v4
    task_id         TEXT    NOT NULL
                    REFERENCES agent_tasks(id) ON DELETE CASCADE,
    step_id         TEXT    NOT NULL
                    REFERENCES agent_steps(id) ON DELETE CASCADE,
    rollback_op     TEXT    NOT NULL              -- reverse operation to apply
                    CHECK (rollback_op IN (
                        'delete_created',          -- undo file_write (delete the created file)
                        'move_back',               -- undo file_move (move src back from dest)
                        'restore_backup',          -- undo file_write/overwrite (restore backup)
                        'delete_created_dir'       -- undo mkdir
                    )),
    params_json     TEXT    NOT NULL,             -- paths / params for the rollback op
    status          TEXT    NOT NULL
                    CHECK (status IN ('pending', 'done', 'failed', 'skipped')),
    created_at      INTEGER NOT NULL,             -- when the forward op executed (before undo needed)
    executed_at     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_rollback_log_task ON rollback_log(task_id, created_at DESC);
```

---

### `agent_personas`

Cache of loaded TOML persona configs (runtime reference table).

```sql
CREATE TABLE IF NOT EXISTS agent_personas (
    id              TEXT    PRIMARY KEY,          -- matches persona.id in TOML
    name            TEXT    NOT NULL,
    toml_path       TEXT    NOT NULL,             -- absolute path to .toml file
    version         TEXT    NOT NULL,
    system_prompt   TEXT    NOT NULL,             -- cached from TOML for quick access
    loaded_at       INTEGER NOT NULL
);
```

---

## Relationships with App 1 Tables (Read-Only)

| App 2 access pattern | App 1 table | Purpose |
|---------------------|-------------|---------|
| SELECT id, name FROM sources WHERE ... | `sources` | Inject indexed document context into ReAct prompt |
| SELECT content FROM chunks WHERE source_id = ? | `chunks` | Retrieve notebook knowledge for task context |

No foreign keys are declared across the App 1/App 2 boundary — soft references only.

---

## Key Validation Rules

- `agent_tasks.status` must follow the defined state machine (enforced in Rust, not DB)
- `agent_steps.reversibility` = `'reversible'` → a `rollback_log` row MUST be inserted before the step executes
- `pending_actions.status` = `'awaiting'` → the parent `agent_tasks.status` MUST be `'awaiting_approval'`
- All timestamps are Unix epoch milliseconds (INTEGER) for consistency with App 1 schema
- All IDs are UUID v4 TEXT (same as App 1 pattern)
