-- ============================================================
-- EchoAI — Initial schema (migration 001)
-- Storage: SQLite WAL + FTS5 + sqlite-vec extension (vec0)
-- ============================================================

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ─── db_config (key-value store for app-level settings) ──────────────────────
CREATE TABLE IF NOT EXISTS db_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ─── notebooks ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS notebooks (
    id         TEXT PRIMARY KEY,          -- UUID v4
    name       TEXT NOT NULL,
    created_at INTEGER NOT NULL,          -- Unix timestamp (ms)
    updated_at INTEGER NOT NULL
);

-- ─── sources ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sources (
    id               TEXT PRIMARY KEY,    -- UUID v4
    file_path        TEXT NOT NULL UNIQUE,
    file_name        TEXT NOT NULL,
    format           TEXT NOT NULL
                     CHECK (format IN ('pdf','docx','xlsx','pptx','txt','md',
                                       'png','jpg','jpeg','gif','webp','bmp',
                                       'mp3','mp4','code','audio','video','image')),
    file_size_bytes  INTEGER NOT NULL,
    file_hash        TEXT NOT NULL,       -- SHA-256
    indexing_status  TEXT NOT NULL DEFAULT 'pending'
                     CHECK (indexing_status IN ('pending','processing','indexed','failed')),
    error_reason     TEXT,
    last_modified    INTEGER NOT NULL,    -- OS mtime (ms)
    indexed_at       INTEGER,
    created_at       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sources_status ON sources(indexing_status);
CREATE INDEX IF NOT EXISTS idx_sources_hash   ON sources(file_hash);

-- ─── notebook_sources (join table) ───────────────────────────────────────────
CREATE TABLE IF NOT EXISTS notebook_sources (
    notebook_id TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    source_id   TEXT NOT NULL REFERENCES sources(id)   ON DELETE CASCADE,
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (notebook_id, source_id)
);
CREATE INDEX IF NOT EXISTS idx_notebook_sources_notebook ON notebook_sources(notebook_id);
CREATE INDEX IF NOT EXISTS idx_notebook_sources_source   ON notebook_sources(source_id);

-- ─── indexed_folders ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS indexed_folders (
    id              TEXT PRIMARY KEY,
    folder_path     TEXT NOT NULL UNIQUE,
    indexing_status TEXT NOT NULL DEFAULT 'pending'
                    CHECK (indexing_status IN ('pending','scanning','indexed','partial')),
    file_count      INTEGER NOT NULL DEFAULT 0,
    last_scanned_at INTEGER,
    created_at      INTEGER NOT NULL
);

-- ─── chunks ──────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS chunks (
    id          TEXT PRIMARY KEY,         -- UUID v4
    source_id   TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,         -- 0-based position within source
    content     TEXT NOT NULL,            -- raw text (≤ 512 tokens)
    char_start  INTEGER NOT NULL,
    char_end    INTEGER NOT NULL,
    page_number INTEGER,                  -- NULL for non-paged formats
    token_count INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chunks_source ON chunks(source_id);

-- ─── chunks_fts (Tier 1 — FTS5 keyword search) ───────────────────────────────
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    content,
    content='chunks',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS5 in sync with chunks
CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, content) VALUES (new.rowid, new.content);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content)
    VALUES ('delete', old.rowid, old.content);
    INSERT INTO chunks_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- ─── chunk_embeddings (Tier 2 — vec0 virtual table, created dynamically) ─────
-- NOTE: This table is NOT created here because the embedding dimension (768 or 384)
-- is determined at runtime by hardware detection.  schema.rs creates it with the
-- correct DIM after reading db_config.embedding_dim.

-- ─── query_cache (Tier 0 — semantic cache) ───────────────────────────────────
CREATE TABLE IF NOT EXISTS query_cache (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    query_text      TEXT NOT NULL,
    query_embedding BLOB NOT NULL,        -- serialised float32 array
    answer_text     TEXT NOT NULL,
    citations_json  TEXT NOT NULL,        -- JSON: [{source_id,chunk_id,excerpt}]
    tier_used       TEXT NOT NULL,
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cache_session ON query_cache(session_id);
CREATE INDEX IF NOT EXISTS idx_cache_created ON query_cache(created_at);

-- ─── query_sessions ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS query_sessions (
    id          TEXT PRIMARY KEY,
    notebook_id TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_notebook ON query_sessions(notebook_id);

-- ─── query_messages ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS query_messages (
    id             TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL REFERENCES query_sessions(id) ON DELETE CASCADE,
    role           TEXT NOT NULL CHECK (role IN ('user','assistant')),
    content        TEXT NOT NULL,
    citations_json TEXT,
    tier_used      TEXT,
    created_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON query_messages(session_id);

-- ─── entities ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS entities (
    id               TEXT PRIMARY KEY,
    canonical_value  TEXT NOT NULL,
    entity_type      TEXT NOT NULL
                     CHECK (entity_type IN ('person','org','date','project','decision','amount','email')),
    normalized_value TEXT,
    source_count     INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL,
    UNIQUE(canonical_value, entity_type)
);
CREATE INDEX IF NOT EXISTS idx_entities_type  ON entities(entity_type);
CREATE INDEX IF NOT EXISTS idx_entities_value ON entities(canonical_value);

-- ─── entity_mentions ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS entity_mentions (
    entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    chunk_id     TEXT NOT NULL REFERENCES chunks(id)   ON DELETE CASCADE,
    context_text TEXT,
    PRIMARY KEY (entity_id, chunk_id)
);
CREATE INDEX IF NOT EXISTS idx_mentions_entity ON entity_mentions(entity_id);
CREATE INDEX IF NOT EXISTS idx_mentions_chunk  ON entity_mentions(chunk_id);

-- ─── entity_relationships ────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS entity_relationships (
    id                TEXT PRIMARY KEY,
    from_entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_entity_id      TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    source_chunk_id   TEXT REFERENCES chunks(id) ON DELETE SET NULL,
    created_at        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rel_from ON entity_relationships(from_entity_id);
CREATE INDEX IF NOT EXISTS idx_rel_to   ON entity_relationships(to_entity_id);

-- ─── batch_jobs ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS batch_jobs (
    id                    TEXT PRIMARY KEY,
    folder_path           TEXT NOT NULL,
    operation             TEXT NOT NULL
                          CHECK (operation IN ('summarize','extract_dates','flag_keywords','extract_actions')),
    operation_params_json TEXT,
    status                TEXT NOT NULL DEFAULT 'queued'
                          CHECK (status IN ('queued','running','complete','partial')),
    total_files           INTEGER NOT NULL DEFAULT 0,
    processed_files       INTEGER NOT NULL DEFAULT 0,
    failed_files          INTEGER NOT NULL DEFAULT 0,
    created_at            INTEGER NOT NULL,
    completed_at          INTEGER
);

-- ─── batch_results ───────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS batch_results (
    id           TEXT PRIMARY KEY,
    job_id       TEXT NOT NULL REFERENCES batch_jobs(id) ON DELETE CASCADE,
    source_id    TEXT REFERENCES sources(id) ON DELETE SET NULL,
    file_path    TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('success','failed')),
    output_text  TEXT,
    error_reason TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_batch_results_job ON batch_results(job_id);

-- ─── audit_log ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS audit_log (
    id           TEXT PRIMARY KEY,
    event_type   TEXT NOT NULL,
    entity_id    TEXT,
    details_json TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_event   ON audit_log(event_type);
