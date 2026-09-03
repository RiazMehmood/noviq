-- ============================================================
-- EchoAI — Schema fixes, round 2 (migration 003)
-- Finishes the work deferred in migration 002: actually widen the
-- CHECK constraints on sources.indexing_status and
-- indexed_folders.indexing_status so the app can use the states it
-- has been writing all along.
--
-- Adds:
--   sources.indexing_status         += 'unindexed'   (used by remove_folder_source)
--   indexed_folders.indexing_status += 'processing'  (used by add_indexed_folder)
--
-- SQLite cannot ALTER a CHECK constraint, so we use the standard
-- table-recreate recipe from https://sqlite.org/lang_altertable.html.
--
-- FK-safety: this whole script runs inside sqlx's per-migration
-- transaction. SQLite enforces foreign keys on INSERT/UPDATE/DELETE of
-- rows, not on DROP TABLE. Since every row is copied verbatim (same id
-- PKs) into the replacement table before DROP and the table is renamed
-- to the original name before commit, the FK graph observed by
-- chunks / notebook_sources / entity_mentions never sees a missing
-- parent — the rename restores the referenced name atomically.
-- ============================================================

-- ── sources ─────────────────────────────────────────────────────────────────
CREATE TABLE sources_new (
    id               TEXT PRIMARY KEY,
    file_path        TEXT NOT NULL UNIQUE,
    file_name        TEXT NOT NULL,
    format           TEXT NOT NULL
                     CHECK (format IN ('pdf','docx','xlsx','pptx','txt','md',
                                       'png','jpg','jpeg','gif','webp','bmp',
                                       'mp3','mp4','code','audio','video','image')),
    file_size_bytes  INTEGER NOT NULL,
    file_hash        TEXT NOT NULL,
    indexing_status  TEXT NOT NULL DEFAULT 'pending'
                     CHECK (indexing_status IN ('pending','processing','indexed','failed','unindexed')),
    error_reason     TEXT,
    last_modified    INTEGER NOT NULL,
    indexed_at       INTEGER,
    created_at       INTEGER NOT NULL
);

INSERT INTO sources_new
SELECT id, file_path, file_name, format, file_size_bytes, file_hash,
       indexing_status, error_reason, last_modified, indexed_at, created_at
FROM sources;

DROP TABLE sources;
ALTER TABLE sources_new RENAME TO sources;

CREATE INDEX IF NOT EXISTS idx_sources_status ON sources(indexing_status);
CREATE INDEX IF NOT EXISTS idx_sources_hash   ON sources(file_hash);

-- ── indexed_folders ─────────────────────────────────────────────────────────
CREATE TABLE indexed_folders_new (
    id              TEXT PRIMARY KEY,
    folder_path     TEXT NOT NULL UNIQUE,
    indexing_status TEXT NOT NULL DEFAULT 'pending'
                    CHECK (indexing_status IN ('pending','scanning','processing','indexed','partial')),
    file_count      INTEGER NOT NULL DEFAULT 0,
    last_scanned_at INTEGER,
    created_at      INTEGER NOT NULL
);

INSERT INTO indexed_folders_new
SELECT id, folder_path, indexing_status, file_count, last_scanned_at, created_at
FROM indexed_folders;

DROP TABLE indexed_folders;
ALTER TABLE indexed_folders_new RENAME TO indexed_folders;
