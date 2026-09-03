# Phase 0 Research: PostgreSQL + pgvector Re-platform

**Feature**: 011-postgres-pgvector-port | **Date**: 2026-06-11

## R1. How does the engine ship inside a desktop exe (no Docker, no install, offline)?

**Decision**: `postgresql_embedded` crate (theseus-rs) with the **`bundled`** feature.
The PostgreSQL 17.x archive (theseus-rs/postgresql-binaries builds, linux/macos/
windows) is downloaded **at compile time** and embedded into the application binary;
at first run the app extracts it, runs initdb into `{app_data_dir}/pgdata/`, and
starts/stops the server as a managed child process. End users install one exe;
runtime is fully offline (Principle I).

**Rationale**: purpose-built for exactly this; actively maintained (releases through
Feb 2026); cross-platform binaries match our Linux-dev/Windows-ship reality; the
crate also handles "is it already running", port selection, and password/auth setup.

**Alternatives considered**:
- *Docker*: fine for servers, dead on arrival for officer desktops (the user's
  question that triggered this research). Rejected.
- *pg-embed crate*: older, less maintained; its `install_extension(path)` API is a
  useful pattern reference but the project is stale relative to postgresql_embedded.
- *Silent EDB installer / system PostgreSQL*: admin rights, Windows services,
  version conflicts — violates zero-setup (FR-004). Rejected.
- *Hand-rolled portable binaries + sidecar scripts*: re-implements what
  postgresql_embedded already does, with more lifecycle bugs. Rejected.

**Risk + spike**: T-first task must prototype init→start→`CREATE EXTENSION vector`
→stop on this machine with networking disabled, to prove the offline claim
end-to-end before any port work begins.

## R2. How does pgvector get into the bundled engine offline?

**Decision**: **Vendor the pgvector extension artifacts in the app package** and
install them into the bundled engine's `lib/` + `share/extension/` directories at
provisioning time, then `CREATE EXTENSION vector`. Companion crate
`postgresql_extensions` (same theseus-rs family) is the first implementation
candidate; if its default flow wants network at runtime, we pre-fetch at build time
and point it at the vendored artifacts (the pg-embed `install_extension(path)`
pattern proves file-based install works — pgvector is a single C extension:
one shared library + control/SQL files).

**Rationale**: keeps runtime fully offline; pgvector is small (<1 MB); per-platform
artifacts are pinned to the bundled PG major version at build time.

**Alternatives considered**: compiling pgvector from source on the user machine
(needs toolchain — absurd for officers); pgvecto.rs/VectorChord (more deps, less
maturity; pgvector is the boring, correct default at this scale).

## R3. Vector index: HNSW config and precision (FR-002, FR-014)

**Decision**: column stays **`vector(768)` full-precision f32** (bit-faithful
migration, FR-007). ANN index is **HNSW with halfvec expression**:
`CREATE INDEX ... USING hnsw ((embedding::halfvec(768)) halfvec_cosine_ops)
WITH (m = 16, ef_construction = 64);` queries order by
`embedding::halfvec(768) <=> $1::halfvec(768)`. Runtime `hnsw.ef_search` tuned per
hardware tier (start 40; raise if SC-005 parity < 95%).

**Rationale**: halfvec *index* halves index size and build time while the stored
column remains exact — so FR-014's "reduced precision only if parity holds" is
satisfiable by *re-indexing only* (swap to full-precision ops) without touching
data. Cosine matches the existing `vec_distance_cosine` semantics in
`retrieval.rs`. HNSW over IVFFlat: no training step, robust recall under
incremental inserts (indexing keeps appending chunks).

**Alternatives considered**: IVFFlat (needs retrain as corpus grows); halfvec
*storage* (lossy migration, violates bit-faithful FR-007 — rejected for v1, can be
revisited once parity data exists); pgvectorscale StreamingDiskANN (extra
extension, overkill below ~50 M vectors; noted for the LAN/server phase).

## R4. FTS5 → PostgreSQL full-text (FR-003)

**Decision**: `tsvector` **stored generated column** on chunk text
(`to_tsvector('english', text)`) + **GIN index**; queries use
`websearch_to_tsquery` (closest to FTS5 query syntax users/code already produce)
with `ts_rank_cd` for scoring. The RRF fusion + S1/S2/S3 boost stack consumes
(rowid, rank) pairs and is engine-agnostic — unchanged downstream (FR-003).

**Rationale**: 1:1 functional replacement, index maintained automatically by the
generated column (no trigger code). FTS index is **rebuilt, not migrated** (spec
assumption): chunk text carries over, generated column populates on insert.

**Alternatives considered**: pg_search/pg_bm25 (extra extension for marginal
ranking gains; can be a later experiment — RRF dampens ranker differences);
trigram (wrong tool for ranked keyword search).

## R5. Porting surface: sqlx + SQL dialect (FR-009, FR-010)

**Decision**: swap sqlx feature `sqlite` → `postgres`; `SqlitePool` → `PgPool` at
the **single construction point** in `db/mod.rs` (57 referencing files mostly just
pass the pool handle — ADR-0014 pool-handle discipline pays off). Dialect fixes are
enumerable: 9× `INSERT OR REPLACE/IGNORE` (7 files) → `ON CONFLICT ... DO
UPDATE/NOTHING`; 2× `last_insert_rowid()` → `RETURNING id`; `?` placeholders →
`$N` (sqlx requires this for PG); vec0/FTS5 SQL confined to `retrieval.rs` /
`query_router.rs` / `indexer.rs` / `schema.rs` + repositories (15 files grepped).
The 25 SQLite migrations are squashed into one native **PG baseline**
(`pg/migrations/001_baseline.sql`); the existing `TARGET_SCHEMA_VERSION` runner
carries forward for future additive migrations.

**Rationale**: hard cutover (spec assumption) means no dual-dialect abstraction
layer — smallest viable diff is a mechanical port with the seam keeping
engine-specific SQL quarantined.

**Type mapping**: INTEGER PRIMARY KEY → `BIGINT GENERATED ALWAYS AS IDENTITY`;
TEXT timestamps (chrono RFC3339 strings) → keep TEXT for v1 parity (no behavior
change), revisit later; BLOB → BYTEA; JSON-in-TEXT (`state_json` etc.) → JSONB
(ADR-0014 already mandated versioned `state_json` → JSONB); vec0 virtual table →
`vector(768)` column on `chunk_embeddings`; FTS5 virtual table → generated
tsvector column (no separate table).

## R6. One-time migration transport (FR-007/008, SC-004)

**Decision**: in-app migration service (`services/migration/sqlite_import.rs`):
open old `echo.db` read-only via rusqlite or a second sqlx-sqlite feature *in the
migration module only*; stream table-by-table in batches using PostgreSQL **COPY
(binary)** via sqlx `copy_in_raw` for the big tables (chunk_embeddings dominates);
per-table resume marker + row-count verification; HNSW index built **after** the
vector COPY completes (`maintenance_work_mem` raised for the build) — bulk-load
then index is much faster than indexed inserts. Pre-flight free-disk check
(old DB size × safety factor). Old DB untouched; final report UI lists per-table
source/target counts.

**Rationale**: COPY is the only realistic way to move ~65 GB in ≤ 4 h (SC-004);
index-after-load is standard pgvector bulk practice. Keeping sqlite-read code
inside the migration module avoids a lingering dual-backend.

**Alternatives considered**: external CLI tool (pgloader: no vec0 understanding,
extra runtime dep); dump/restore via CSV (slower, float round-trip risk — binary
COPY preserves f32 exactly, supporting bit-fidelity verification).

## R7. Engine lifecycle + coexistence (FR-005/006/012)

**Decision**: `db/pg/engine.rs` owns: first-run initdb; start on app launch with
**Unix socket (Linux/macOS) / 127.0.0.1 + free ephemeral port (Windows)**,
`listen_addresses` restricted to loopback; generated superuser password stored in
app data dir (0600); health-check before pool construction; PID-file orphan
detection (adopt if healthy & version-matched, else stop+restart); clean shutdown
on Tauri exit hook; crash recovery is PG's own WAL replay. Startup failures map to
plain-language UI errors with one remediation each (port conflict → auto-retry on
new port; corrupt data dir → point at recovery/migration re-run).

**Per-tier tuning** (`db/pg/settings.rs`, Principle VII): shared_buffers 256 MB /
512 MB / 1 GB (tiers ≤2 / 3 / 4+), `max_parallel_workers_per_gather=2`,
`maintenance_work_mem` raised only during migration/index build, `hnsw.ef_search`
per tier. Goal: the engine + warm 3B model + demand 7B stay inside the machine's
RAM without re-creating the oomd scenario.

## R8. Dev/test parity (FR-015, SC-007)

**Decision**: tests provision an **ephemeral bundled instance** (postgresql_embedded
supports temp-dir instances) per test binary, applying the PG baseline; a shared
`test_pg()` helper replaces the current in-memory SQLite helpers. Dev runs use the
same engine path as production. `ECHO_PG_URI` env override = the opt-in external
engine for developer convenience only (never required, never shipped behavior).

**Open items carried to tasks** (not blockers):
- Spike result R1/R2 (offline bundle + vendored pgvector) gates everything — first task.
- Measure live `chunk_embeddings` row count before migration (sizing for SC-004; user runs the inspection command per workflow).
- `ts_rank_cd` vs FTS5 bm25 ranking drift — covered by SC-005 parity run, RRF dampens.
- Windows bundling (installer size +~60–80 MB, AV/signing) — risk logged in spec; validated when the Windows build plan resumes.

**Sources**: [postgresql_embedded (crates.io)](https://crates.io/crates/postgresql_embedded) · [theseus-rs/postgresql-embedded (GitHub)](https://github.com/theseus-rs/postgresql-embedded) · [postgresql_embedded docs](https://docs.rs/postgresql_embedded/latest/postgresql_embedded/) · [postgresql_extensions (crates.io)](https://crates.io/crates/postgresql_extensions) · [pg-embed (crates.io)](https://crates.io/crates/pg-embed)
