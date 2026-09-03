# Data Model: Notebook & Knowledge Engine

**Branch**: `001-notebook-knowledge-engine` | **Date**: 2026-04-13
**Storage**: SQLite (WAL mode) + sqlite-vec extension + FTS5 virtual tables
**Driver**: sqlx (async Rust)

---

## Entity Relationship Overview

```
Notebook ──< NotebookSource >── Source ──< Chunk ──< ChunkEmbedding (vec0)
                                  │          │
                              IndexedFolder  ChunkFTS (fts5)
                                             │
                                        EntityMention >── Entity ──< Relationship
                                             │
                                         QueryCache

QuerySession ──< QueryMessage
    │
  Notebook

BatchJob ──< BatchResult ── Source
AuditLog
```

---

## Schema

### `notebooks`
```sql
CREATE TABLE notebooks (
    id          TEXT PRIMARY KEY,          -- UUID v4
    name        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,          -- Unix timestamp (ms)
    updated_at  INTEGER NOT NULL
);
```

### `notebook_sources` (join table)
```sql
CREATE TABLE notebook_sources (
    notebook_id  TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    source_id    TEXT NOT NULL REFERENCES sources(id)   ON DELETE CASCADE,
    added_at     INTEGER NOT NULL,
    PRIMARY KEY (notebook_id, source_id)
);
CREATE INDEX idx_notebook_sources_notebook ON notebook_sources(notebook_id);
CREATE INDEX idx_notebook_sources_source   ON notebook_sources(source_id);
```

### `sources`
```sql
CREATE TABLE sources (
    id               TEXT PRIMARY KEY,     -- UUID v4
    file_path        TEXT NOT NULL UNIQUE, -- absolute path; dedup key
    file_name        TEXT NOT NULL,
    format           TEXT NOT NULL,        -- 'pdf'|'docx'|'xlsx'|'pptx'|'txt'|
                                           --  'md'|'png'|'jpg'|'mp3'|'mp4'|'code'
    file_size_bytes  INTEGER NOT NULL,
    file_hash        TEXT NOT NULL,        -- SHA-256 of file contents
    indexing_status  TEXT NOT NULL         -- 'pending'|'processing'|'indexed'|'failed'
                     DEFAULT 'pending',
    error_reason     TEXT,                 -- populated when status = 'failed'
    last_modified    INTEGER NOT NULL,     -- OS mtime (ms)
    indexed_at       INTEGER,              -- set when status → 'indexed'
    created_at       INTEGER NOT NULL
);
CREATE INDEX idx_sources_status ON sources(indexing_status);
CREATE INDEX idx_sources_hash   ON sources(file_hash);
```

**State transitions**: `pending` → `processing` → `indexed` | `failed`
Re-index: any state → `processing` (triggered by file-change event or import mismatch)

### `indexed_folders`
```sql
CREATE TABLE indexed_folders (
    id               TEXT PRIMARY KEY,
    folder_path      TEXT NOT NULL UNIQUE,
    indexing_status  TEXT NOT NULL DEFAULT 'pending',  -- 'pending'|'scanning'|'indexed'|'partial'
    file_count       INTEGER NOT NULL DEFAULT 0,
    last_scanned_at  INTEGER,
    created_at       INTEGER NOT NULL
);
```

### `chunks`
```sql
CREATE TABLE chunks (
    id           TEXT PRIMARY KEY,         -- UUID v4
    source_id    TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,         -- 0-based position within source
    content      TEXT NOT NULL,            -- raw text (512 tokens max)
    char_start   INTEGER NOT NULL,         -- character offset in source text
    char_end     INTEGER NOT NULL,
    page_number  INTEGER,                  -- PDF page; NULL for non-paged formats
    token_count  INTEGER NOT NULL,
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_chunks_source ON chunks(source_id);
```

### `chunks_fts` (Tier 1 — keyword search)
```sql
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content,
    content='chunks',
    content_rowid='rowid',
    tokenize='porter unicode61'
);
-- Triggers to keep FTS in sync
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, content) VALUES (new.rowid, new.content);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
END;
CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
    INSERT INTO chunks_fts(rowid, content) VALUES (new.rowid, new.content);
END;
```

### `chunk_embeddings` (Tier 2 — semantic search via sqlite-vec)
```sql
-- nomic-embed-text-v1.5: 768 dimensions
-- Downgrade to 384 dims (MiniLM) on 8 GB hardware; schema version tracks this
CREATE VIRTUAL TABLE chunk_embeddings USING vec0(
    chunk_id  TEXT PARTITION KEY,
    embedding FLOAT[768]
);
```

### `query_cache` (Tier 0 — semantic cache)
```sql
CREATE TABLE query_cache (
    id               TEXT PRIMARY KEY,
    session_id       TEXT NOT NULL,        -- owner session
    query_text       TEXT NOT NULL,
    query_embedding  BLOB NOT NULL,        -- serialized float32 array
    answer_text      TEXT NOT NULL,
    citations_json   TEXT NOT NULL,        -- JSON: [{source_id, chunk_id, excerpt}]
    tier_used        TEXT NOT NULL,        -- tier that produced the original answer
    created_at       INTEGER NOT NULL
);
CREATE INDEX idx_cache_session   ON query_cache(session_id);
CREATE INDEX idx_cache_created   ON query_cache(created_at);
-- 7-day retention enforced by cleanup job
```

### `query_sessions`
```sql
CREATE TABLE query_sessions (
    id          TEXT PRIMARY KEY,
    notebook_id TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX idx_sessions_notebook ON query_sessions(notebook_id);
```

### `query_messages`
```sql
CREATE TABLE query_messages (
    id             TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL REFERENCES query_sessions(id) ON DELETE CASCADE,
    role           TEXT NOT NULL,           -- 'user' | 'assistant'
    content        TEXT NOT NULL,
    citations_json TEXT,                    -- JSON: [{source_id, chunk_id, excerpt, page}]
    tier_used      TEXT,                    -- 'cache'|'fts'|'vec'|'tier3'|'tier4'
    created_at     INTEGER NOT NULL
);
CREATE INDEX idx_messages_session ON query_messages(session_id);
```

### `entities`
```sql
CREATE TABLE entities (
    id               TEXT PRIMARY KEY,
    canonical_value  TEXT NOT NULL,
    entity_type      TEXT NOT NULL,   -- 'person'|'org'|'date'|'project'|'decision'|'amount'
    normalized_value TEXT,            -- ISO date for dates; canonical casing for names
    source_count     INTEGER NOT NULL DEFAULT 0,  -- cached mention count
    created_at       INTEGER NOT NULL,
    UNIQUE(canonical_value, entity_type)
);
CREATE INDEX idx_entities_type  ON entities(entity_type);
CREATE INDEX idx_entities_value ON entities(canonical_value);
```

### `entity_mentions`
```sql
CREATE TABLE entity_mentions (
    entity_id     TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    chunk_id      TEXT NOT NULL REFERENCES chunks(id)   ON DELETE CASCADE,
    context_text  TEXT,                   -- ~100 chars surrounding the mention
    PRIMARY KEY (entity_id, chunk_id)
);
CREATE INDEX idx_mentions_entity ON entity_mentions(entity_id);
CREATE INDEX idx_mentions_chunk  ON entity_mentions(chunk_id);
```

### `entity_relationships`
```sql
CREATE TABLE entity_relationships (
    id                TEXT PRIMARY KEY,
    from_entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,   -- 'attended'|'decided'|'involved_in'|'owns'|'reported_to'
    source_chunk_id   TEXT REFERENCES chunks(id) ON DELETE SET NULL,
    created_at        INTEGER NOT NULL
);
CREATE INDEX idx_rel_from ON entity_relationships(from_entity_id);
CREATE INDEX idx_rel_to   ON entity_relationships(to_entity_id);
```

### `batch_jobs`
```sql
CREATE TABLE batch_jobs (
    id                   TEXT PRIMARY KEY,
    folder_path          TEXT NOT NULL,
    operation            TEXT NOT NULL,    -- 'summarize'|'extract_dates'|'flag_keywords'|'extract_actions'
    operation_params_json TEXT,            -- e.g. {"keywords": ["penalty", "deadline"]}
    status               TEXT NOT NULL DEFAULT 'queued',  -- 'queued'|'running'|'complete'|'partial'
    total_files          INTEGER NOT NULL DEFAULT 0,
    processed_files      INTEGER NOT NULL DEFAULT 0,
    failed_files         INTEGER NOT NULL DEFAULT 0,
    created_at           INTEGER NOT NULL,
    completed_at         INTEGER
);
```

### `batch_results`
```sql
CREATE TABLE batch_results (
    id            TEXT PRIMARY KEY,
    job_id        TEXT NOT NULL REFERENCES batch_jobs(id) ON DELETE CASCADE,
    source_id     TEXT REFERENCES sources(id) ON DELETE SET NULL,
    file_path     TEXT NOT NULL,
    status        TEXT NOT NULL,          -- 'success' | 'failed'
    output_text   TEXT,
    error_reason  TEXT,
    created_at    INTEGER NOT NULL
);
CREATE INDEX idx_batch_results_job ON batch_results(job_id);
```

### `audit_log`
```sql
CREATE TABLE audit_log (
    id           TEXT PRIMARY KEY,
    event_type   TEXT NOT NULL,     -- 'file_indexed'|'query_answered'|'data_deleted'|
                                    --  'folder_revoked'|'batch_completed'|'source_failed'
    entity_id    TEXT,              -- related record ID (nullable)
    details_json TEXT,              -- event-specific context
    created_at   INTEGER NOT NULL
);
CREATE INDEX idx_audit_created   ON audit_log(created_at);
CREATE INDEX idx_audit_event     ON audit_log(event_type);
```

---

## Key Constraints & Rules

| Rule | Enforcement |
|------|-------------|
| One index entry per file path | `sources.file_path UNIQUE` constraint |
| Source shared across contexts | `notebook_sources` join; no source duplication |
| State machine integrity | App-layer; SQLite CHECK constraint: `indexing_status IN ('pending','processing','indexed','failed')` |
| No external network calls | App-layer; no schema enforces this |
| Semantic cache 7-day TTL | Cleanup job deletes `query_cache` where `created_at < now - 7d` |
| Knowledge graph max 500 entities in UI | Frontend pagination; no DB limit |
| Entity uniqueness | `UNIQUE(canonical_value, entity_type)` |
| Audit completeness | Insert triggers on sources, query_messages |

---

## Migration Strategy

Migrations use numbered SQL files in `src-tauri/src/db/migrations/`:
- `001_initial_schema.sql` — all tables above
- `002_*` — future amendments

sqlx's `migrate!()` macro runs migrations at app startup. Schema version stored
in SQLite's `user_version` pragma. Import of older archives triggers migration
of the archive's `db.sqlite` before merging into the main database.
