# Implementation Plan: PostgreSQL + pgvector Persistence Re-platform (Single-Machine)

**Branch**: `011-postgres-pgvector-port` | **Date**: 2026-06-11 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/011-postgres-pgvector-port/spec.md`

## Summary

Replace SQLite (+ sqlite-vec vec0 + FTS5) with an **app-bundled PostgreSQL 17 +
pgvector** instance as the single source of truth, in single-machine mode only.
Driver: vec0 KNN is brute-force — 1247–1678 s per lookup on the live 65 GB
`chunk_embeddings`, page-cache thrash, systemd-oomd kill (010 T035 gate fails
~100×). pgvector's HNSW index makes the same lookup sub-second. The engine ships
*inside* the Tauri app (`postgresql_embedded` crate, `bundled` feature — binaries
embedded in the exe at build time; pgvector extension vendored in the package), is
lifecycle-managed as a child process, loopback-only. One-time streaming migration
carries the corpus over with zero re-embedding. Constitution §IV and the stack
invariants are formally amended via ADR before cutover (FR-013).

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`) + TypeScript/SolidJS, Tauri 2.0
**Primary Dependencies**: sqlx 0.8 (swap `sqlite` → `postgres` feature; keep `runtime-tokio`, `macros` off compile-time checks — runtime queries only), `postgresql_embedded` (theseus-rs, `bundled` feature, PG 17.x), pgvector ≥ 0.8 (vendored extension artifacts installed into the bundled engine offline), `pgvector`-rust or raw `vector` casts for bind/decode, existing fastembed 4 / llama-cpp-2 untouched
**Storage**: Bundled PostgreSQL 17 + pgvector; data dir under `{app_data_dir}/pgdata/`; loopback/Unix-socket only; old `echo.db` retained read-only until user confirms migration
**Testing**: cargo test (lib + integration) against an app-provisioned ephemeral PG instance (same `postgresql_embedded` path as production — FR-015); existing suites (drafting 70+, query_router 50+, acceptance gates) must pass unmodified in behavior (SC-007); new `pg_parity` + `migration_verify` integration tests
**Target Platform**: Linux dev now; Windows .exe is the distribution target — bundling path must work on both (theseus binaries cover linux/macos/windows)
**Project Type**: Single Tauri project (`src-tauri` backend + SolidJS frontend); persistence-layer feature, near-zero frontend surface (migration progress UI only)
**Performance Goals**: research-stage KNN lookup ≤ 15 s on live corpus (SC-001, target: sub-second via HNSW); one-ask draft completes without oomd kill ×10 (SC-002); indexing throughput within ±10% (SC-006); migration of 65 GB ≤ 4 h unattended (SC-004)
**Constraints**: fully offline at runtime (Principle I — `bundled` feature embeds binaries at *compile* time; no runtime downloads); engine must coexist with a foreign PostgreSQL on the machine (own port/socket, own data dir); RAM budget shared with single model slot (Principle III) — engine tuned small (shared_buffers ≤ 512 MB–1 GB adaptive per hardware tier, Principle VII); additive-migration discipline continues on the new engine
**Scale/Scope**: live DB 65 GB (~95% `chunk_embeddings`, 768-d f32 vectors); 25 SQLite migrations re-expressed as one PG baseline; 57 files reference `SqlitePool` (pool-type swap at one construction point + per-module dialect fixes: 9 `INSERT OR REPLACE` in 7 files, 2 `last_insert_rowid`, FTS5/vec0 SQL in 15 files)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Privacy-First, Offline-Always | ✅ PASS | Engine bundled in the app, runs locally, loopback-only (FR-006); zero runtime network/downloads (`bundled` = compile-time embed). |
| II | Tiered Intelligence | ✅ PASS | Tier ladder unchanged; Tier 2 (ANN semantic) becomes *actually* ANN. |
| III | Single Model Slot | ✅ PASS | No LLM impact. PG process adds bounded RAM (tuned ≤ 1 GB); net memory pressure *drops* (no more 65 GB page-cache thrash). |
| IV | SQLite as Single Source of Truth | ❌ **VIOLATION — amendment required** | This feature's entire point. Gate: ADR + constitution amendment (v2.0.0, §IV redefined to "single app-managed local store (bundled PostgreSQL)") MUST be accepted before cutover lands on master (FR-013). See Complexity Tracking. |
| V | Pending Actions Queue | ✅ PASS | Untouched. |
| VI | Persona-as-Config | ✅ PASS | Untouched. |
| VII | Hardware-Adaptive | ✅ PASS | Engine memory settings selected per hardware tier; startup failure → plain-language error + remediation (FR-012). |
| VIII | Incremental Feature-App Delivery | ✅ PASS | P1 story alone (fast research on dev machine) is a usable MVP. |
| IX–XIII | Domain & Product | ✅ PASS | Retrieval parity gated by SC-005; citation discipline unchanged (verbatim verify gate downstream); graph tables carried 1:1. |
| — | Tech Stack Invariants table | ❌ **VIOLATION — same amendment** | Rows "Vector search: sqlite-vec", "Full-text search: SQLite FTS5", "Database: SQLite via sqlx" change to pgvector / PG tsvector+GIN / "Bundled PostgreSQL via sqlx". Same ADR + amendment covers it (deviation rule in constitution: amendment with ADR). |

**Gate verdict**: PROCEED — the two violations are the feature itself, pre-approved
in direction by the user (pivot plan Phase 0) and tracked as FR-013. Cutover to
master is hard-blocked on the accepted ADR + constitution v2.0.0.

## Project Structure

### Documentation (this feature)

```text
specs/011-postgres-pgvector-port/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── storage-seam.md  # Rust trait/repository contracts (no REST API in this feature)
└── tasks.md             # Phase 2 (/sp.tasks — NOT created by /sp.plan)
```

### Source Code (repository root)

```text
src-tauri/src/
├── db/
│   ├── mod.rs                    # pool construction → becomes PgPool, single construction point
│   ├── schema.rs                 # SQLite migration runner → PG baseline + additive runner
│   ├── pg/
│   │   ├── engine.rs             # NEW: bundled-engine lifecycle (init/start/health/stop/orphan adoption)
│   │   ├── settings.rs           # NEW: per-hardware-tier PG tuning (shared_buffers, workers, hnsw ef_search)
│   │   └── migrations/
│   │       └── 001_baseline.sql  # NEW: all 25 SQLite migrations re-expressed natively
│   └── repositories/             # existing repos: SQL dialect fixes (ON CONFLICT, RETURNING id)
├── services/
│   ├── drafting/retrieval.rs     # vec0 MATCH/k → pgvector `<=>` ORDER BY LIMIT (seam already exists, ADR-0014)
│   ├── query_router.rs           # FTS5 MATCH → websearch_to_tsquery + ts_rank; RRF fusion unchanged
│   ├── indexer.rs                # vec0/FTS writes → vector column + tsvector generated column
│   └── migration/                # NEW: one-time echo.db → PG streaming migration (resume, verify, report)
│       └── sqlite_import.rs
└── commands/
    └── migration.rs              # NEW: Tauri cmds + progress events for the migration UI

src/components/settings/MigrationPanel.tsx   # NEW: one-time migration progress/report UI
src/components/StartupError.tsx              # NEW: plain-language engine-startup errors + remediation (FR-012)

src-tauri/tests/
├── pg_parity.rs                  # NEW: top-K overlap old-vs-new on eval set (SC-005)
└── migration_verify.rs           # NEW: row counts + vector bit-fidelity on fixture DB
```

**Structure Decision**: single-project layout retained. The port is a pool-type
swap at the one construction point in `db/mod.rs` plus per-module dialect fixes —
NOT a parallel backend (hard cutover per spec assumption). New code clusters in
`db/pg/` (engine lifecycle) and `services/migration/`. ADR-0014 seam discipline:
vector SQL stays inside `retrieval.rs`; FTS SQL stays inside `query_router.rs`
helpers; no engine-specific SQL leaks into command handlers.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Constitution §IV (no Postgres) | vec0 is brute-force; 1678 s/lookup + oomd kill on live corpus makes the flagship drafting feature unusable; sqlite-vec 0.1.x has no ANN index | (a) Pre-filtering/partitioning vec0: still O(corpus) per query, no index; (b) usearch/hnswlib sidecar index next to SQLite: second source of truth for vectors, custom WAL/compaction, still pays SQLite FTS5 + 65 GB bloat, dead-ends before the LAN pivot; (c) wait for sqlite-vec ANN: no release timeline, blocker is now |
| Stack invariants (sqlite-vec/FTS5/SQLite rows) | Same as above; tsvector+GIN replaces FTS5 1:1 within RRF | Keeping FTS5 while moving vectors would split persistence across two engines — worst of both worlds |
| New runtime child process (PG) vs in-process DB | ANN at this scale needs a real index engine; bundled+lifecycle-managed keeps UX identical to SQLite (zero user setup, FR-004/005) | Docker: unshippable to officer desktops (user's exe question); system PG install: violates zero-setup and offline install |
