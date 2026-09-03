# Data Model: PostgreSQL + pgvector Re-platform

**Feature**: 011-postgres-pgvector-port | **Date**: 2026-06-11

## Carried-over entities (1:1, dialect-mapped)

All 25 SQLite migrations' tables are re-expressed in `pg/migrations/001_baseline.sql`.
No semantic changes — this is a port, not a redesign. Global type mapping:

| SQLite | PostgreSQL | Notes |
|--------|------------|-------|
| `INTEGER PRIMARY KEY (AUTOINCREMENT)` | `BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | `last_insert_rowid()` call-sites (2) → `RETURNING id` |
| `TEXT` (RFC3339 timestamps via chrono) | `TEXT` (v1, parity) | TIMESTAMPTZ deferred — no behavior change in v1 |
| `BLOB` | `BYTEA` | |
| JSON-in-`TEXT` (`state_json`, payloads) | `JSONB` | ADR-0014 already mandates versioned `state_json` → JSONB |
| `INSERT OR REPLACE` / `OR IGNORE` | `INSERT ... ON CONFLICT ... DO UPDATE / DO NOTHING` | 9 occurrences, 7 files |
| `?` placeholders | `$1..$N` | mechanical |

Functional groups (source migrations in parentheses):

- **Corpus**: folders, sources/documents, chunks, hierarchical chunks (001–003, 013), extraction quality (016)
- **Vectors**: `chunk_embeddings` — see below (replaces vec0 virtual table)
- **Full-text**: generated `tsv` column on chunk text — replaces FTS5 virtual table (no separate table)
- **Knowledge graph**: graph_nodes/edges, constitution graph, NER progress (008/014/015, 020)
- **Importer/IR**: index_jobs, doc_ir_progress, structured/LLM IR (004, 017–019)
- **Agents/meetings/settings**: agent tables, meetings, trial/gpu settings (006/007, 010–012)
- **Drafting**: template_schema/template_slot(+provision_ref), draft_session, chat_message, conversation_summary, one_ask_session (021–025)

## Changed entity: `chunk_embeddings`

```sql
CREATE TABLE chunk_embeddings (
  chunk_id  BIGINT PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
  embedding vector(768) NOT NULL          -- full-precision f32, bit-faithful from vec0
);
CREATE INDEX chunk_embeddings_hnsw
  ON chunk_embeddings
  USING hnsw ((embedding::halfvec(768)) halfvec_cosine_ops)
  WITH (m = 16, ef_construction = 64);    -- index-only precision cut (FR-014); column stays exact
```

- Query shape (inside `retrieval.rs` seam only):
  `SELECT chunk_id, embedding::halfvec(768) <=> $1::halfvec(768) AS dist
   FROM chunk_embeddings ORDER BY dist LIMIT $2;`
- `hnsw.ef_search` set per hardware tier at session start; raised if SC-005 parity < 95%.
- FK to chunks gives FK-safe wipe semantics matching the 009 re-index fix.

## Changed entity: chunk full-text

```sql
ALTER TABLE chunks ADD COLUMN tsv tsvector
  GENERATED ALWAYS AS (to_tsvector('english', coalesce(text,''))) STORED;
CREATE INDEX chunks_tsv_gin ON chunks USING gin (tsv);
```

- Query shape (inside `query_router.rs` helpers only):
  `WHERE tsv @@ websearch_to_tsquery('english', $1)` ranked by `ts_rank_cd`.
- Rebuilt, not migrated: populates automatically as rows are COPYed in.

## New entity: Bundled engine instance (no DB table — app-data files)

| Field | Where | Purpose |
|-------|-------|---------|
| data dir | `{app_data_dir}/pgdata/` | initdb target; PG owns contents (WAL recovery) |
| socket/port | Unix socket dir (Linux/macOS) / `127.0.0.1:<free port>` (Windows) | loopback-only (FR-006); persisted in `pg-instance.json` |
| superuser credential | `pg-instance.json` (mode 0600) | generated at first init |
| engine version | `pg-instance.json` | orphan adoption check + future engine upgrades |
| pid | PG postmaster.pid | orphan detection (FR-005) |

State transitions: `Absent → Initialized → Running ⇄ Stopped`, with
`Running(orphan) → adopted | replaced` on startup, and any failure →
`Failed(reason, remediation)` surfaced to UI (FR-012).

## New entity: Migration job (PG table `migration_job`, additive)

| Field | Type | Purpose |
|-------|------|---------|
| id | BIGINT IDENTITY PK | one row per attempt |
| source_path | TEXT | old echo.db absolute path |
| source_schema_version | INT | must equal expected version else refuse (edge case) |
| table_progress | JSONB | per-table: rows_copied, source_count, target_count, done |
| phase | TEXT | `preflight → copying → indexing → verifying → done / failed` |
| started_at / finished_at | TEXT | timing for SC-004 |
| report | JSONB | final per-table verification report shown in UI |

Validation rules: pre-flight free-disk ≥ source size × 1.2 (FR-008); resume = skip
tables marked done, truncate+recopy the in-flight table (idempotent, FR-008);
vector bit-fidelity spot-check (N random vectors byte-compared post-COPY).
