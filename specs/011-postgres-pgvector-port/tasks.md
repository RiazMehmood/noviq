# Tasks: PostgreSQL + pgvector Persistence Re-platform (Single-Machine)

**Input**: Design documents from `/specs/011-postgres-pgvector-port/`
**Prerequisites**: plan.md, spec.md, research.md (R1–R8), data-model.md, contracts/storage-seam.md, quickstart.md

**Tests**: included (repo TDD convention; SC-005/SC-007 are test-shaped gates).

**Organization**: grouped by user story. NOTE: unlike a greenfield feature, this is a
hard-cutover port — Phase 2 (engine + baseline + pool swap) is large and BLOCKS
everything; the app does not compile against PG until T013 completes.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup (Spike Gate + Dependencies)

**Purpose**: prove the load-bearing mechanism before any port work (research R1/R2 risk).

- [X] T001 Spike test `src-tauri/tests/pg_spike.rs`: ✅ PASSED 2026-06-12 (PG 17.10 bundled offline; pgvector VENDORED from-source v0.8.1 — portal-corp REJECTED, AssetNotFound on linux-x86_64; vector(768) + halfvec KNN + HNSW index green, 11 s). Original scope: `postgresql_embedded` (bundled feature) initdb → start loopback → install vendored pgvector → `CREATE EXTENSION vector` → insert/query one `vector(768)` → clean stop. Run once with networking disabled to prove runtime-offline. **HARD GATE: if pgvector cannot be installed offline into the bundled engine, STOP and re-plan R2 before proceeding.**
- [X] T002 ✅ COMPLETE 2026-06-12 (sqlite feature removed, postgres-only Cargo.toml verified at T011) (was PARTIAL: postgres feature + postgresql_embedded/extensions/semver added as dev-deps, POSTGRESQL_VERSION pinned =17.10.0 via run scripts; promotion to runtime deps + sqlite removal happens at T008/T010) Add dependencies in `src-tauri/Cargo.toml`: `postgresql_embedded` (bundled), `pgvector` (sqlx support), swap sqlx feature `sqlite` → `postgres`; pin POSTGRESQL_VERSION env in `.cargo/config.toml` or build env (NTFS CARGO_TARGET_DIR rules apply). Expect compile breakage across the crate — do NOT fix call sites yet (Phase 2 does).
- [X] T003 [P] ✅ DONE 2026-06-12 (linux-x86_64 vendored: pgvector 0.8.1 from source vs bundled pg_config, OPTFLAGS="" portable, 327 KB; Windows artifacts owed when Windows build resumes) Vendor pgvector extension artifacts per platform under `src-tauri/resources/pgvector/` + build-time fetch script `src-tauri/build_pgvector.sh`; document pinning (PG major ↔ pgvector version) in the script header.

**Checkpoint**: spike green offline = mechanism proven.

---

## Phase 2: Foundational (Engine + Baseline + Pool Swap) ⚠️ BLOCKS ALL STORIES

**Purpose**: app boots against bundled PG with full schema; crate compiles end-to-end.

- [X] T004 ✅ 2026-06-12 — Engine lifecycle `src-tauri/src/db/pg/engine.rs` per contract C3: `ensure_running(app_data_dir, tier)` (init-if-absent → start → health-check → extension check), `connect`, `shutdown`, `EngineError` variants each carrying `remediation: String`; instance metadata in `{app_data_dir}/pg-instance.json` (0600); Unix socket on Linux/macOS, 127.0.0.1+free port on Windows; `listen_addresses` loopback-only (FR-006).
- [X] T005 ✅ 2026-06-12 — [P] Per-tier tuning `src-tauri/src/db/pg/settings.rs`: shared_buffers 256M/512M/1G by HardwareTier, max_parallel_workers_per_gather=2, hnsw.ef_search per tier (Principle VII/III).
- [X] T006 ✅ 2026-06-12 — PG baseline `src-tauri/src/db/pg/migrations/001_baseline.sql`: all 25 SQLite migrations re-expressed natively per data-model.md type map — IDENTITY PKs, JSONB for `state_json`/JSON payloads, BYTEA, `chunk_embeddings(chunk_id BIGINT PK FK, embedding vector(768))` + halfvec-expression HNSW index (m=16, ef_construction=64), `chunks.tsv` stored generated tsvector + GIN. No FTS5/vec0 virtual tables.
- [X] T007 ✅ 2026-06-12 — Port migration runner `src-tauri/src/db/schema.rs`: apply baseline on fresh DB, keep `TARGET_SCHEMA_VERSION`-style additive runner for future PG migrations; remove sqlite-vec extension loading.
- [X] T008 ✅ 2026-06-12 — Pool swap at the single construction point `src-tauri/src/db/mod.rs`: introduce `pub type Db = sqlx::PgPool`, construct via `pg::engine::ensure_running` + `connect`; wire Tauri startup (`src-tauri/src/lib.rs`) and exit hook → `shutdown`.
- [X] T009 ✅ 2026-06-12 (landed as `db::testing::test_pool()` in src/db/mod.rs — shared OnceCell engine + fresh DB per call, public for integration tests; supersedes planned tests/common/pg.rs) — [P] Test helper `src-tauri/tests/common/pg.rs` per contract C5: `test_pg()` ephemeral bundled instance + baseline applied; same engine path as production (FR-015).
- [X] T010 ✅ 2026-06-12 — Mechanical dialect sweep (compile-driven): `?` → `$N` everywhere; 9× `INSERT OR REPLACE/IGNORE` → `ON CONFLICT` in the 7 files (`grep -rE "INSERT OR (REPLACE|IGNORE)" src-tauri/src`); 2× `last_insert_rowid()` → `RETURNING id`; `SqlitePool` → `Db` alias across 57 files. vec0/FTS5 query bodies stubbed `todo!()`-free but feature-broken is OK here (US1 ports them) — they must compile with plain-SQL placeholders that return empty results, clearly marked `// PORT(US1)`.
- [X] T011 ✅ 2026-06-12 — `cargo check --lib`/`--tests` exit 0; RED list resolved same-day (see T017). Compile-green checkpoint: `cargo check` exit 0; `cargo test --lib` — record which suites fail only on the `// PORT(US1)` placeholders (expected RED list for Phase 3).

**Checkpoint**: app launches, provisions pgdata, schema applied; retrieval features knowingly degraded until US1.

---

## Phase 3: User Story 1 — Drafting research fast, no oomd kill (P1) 🎯 MVP

**Goal**: vec0 brute force → pgvector HNSW; FTS5 → tsvector; live research lookup ≤ 15 s (SC-001), draft completes without kill (SC-002).

**Independent Test**: fresh-index a corpus in dev, run a one-ask draft; lookups ≤ 15 s, journal quiet.

### Tests (RED first)

- [X] T012 ✅ 2026-06-12 — ported to `db::testing::test_pool()`, brute-force ref = `<=>` exact cosine; 2/2 PASS. [P] [US1] Re-point `knn_parity` fixture test at PG via `test_pg()` (order ≡ exact cosine on fixture vectors) — must FAIL before T013, PASS after.
- [X] T013a ✅ 2026-06-12 — covered by `test_query_router::test_fts5_keyword_tier1` (websearch OR-tokens + ts_rank_cd, end-to-end through route()) + `hierarchical_retrieval_test` (phrase match); no separate pg_fts.rs needed. [P] [US1] FTS contract test in `src-tauri/tests/pg_fts.rs`: keyword_topk returns ranked hits for fixture chunks (websearch syntax: quoted phrase + OR) — RED before T015.

### Implementation

- [X] T013 ✅ 2026-06-12 — halfvec-expression KNN + ef_search set per-connection (engine.rs after_connect). [US1] Port vector seam `src-tauri/src/services/drafting/retrieval.rs` (contract C1): `SELECT chunk_id, embedding::halfvec(768) <=> $1::halfvec(768) AS dist ... ORDER BY dist LIMIT $2` with existing `RetrievalScope` filters; set `hnsw.ef_search` from settings at pool/session init.
- [X] T014 ✅ 2026-06-12 — websearch_to_tsquery + ts_rank_cd, RRF untouched. [US1] Port FTS seam in `src-tauri/src/services/query_router.rs` keyword helpers (contract C2): `tsv @@ websearch_to_tsquery('english',$1)` + `ts_rank_cd`; RRF fusion + S1/S2/S3 boost stack untouched (FR-003).
- [X] T015 ✅ 2026-06-12 — ON CONFLICT (chunk_id) DO UPDATE vector upsert; tsv generated. [US1] Port indexer writes `src-tauri/src/services/indexer.rs`: embedding upsert → `INSERT ... ON CONFLICT (chunk_id) DO UPDATE` into vector column; delete FTS5/vec0 upsert paths (tsv is generated); keep FK-safe wipe semantics from the 009 re-index fix.
- [X] T016 ✅ 2026-06-12 — swept; 5 full-precision `<=>` sites (lib.rs warmup, search.rs, drafting.rs ×3) re-aligned to the halfvec index expression. [US1] Sweep remaining vec0/FTS5 call sites (the `// PORT(US1)` placeholders from T010) in: `src-tauri/src/commands/search.rs`, `commands/folders.rs`, `commands/drafting.rs`, `services/knowledge_graph.rs`, `services/analysis_engine.rs`, `services/structure_parser.rs`, `services/intent_router/mod.rs`, `services/drafting/mod.rs`, `db/repositories/chunks.rs`, `db/repositories/folders.rs` — route vector SQL through retrieval.rs, FTS through query_router helpers (FR-009 seam audit comes in US4).
- [X] T017 ✅ 2026-06-12 — lib 368/370 (2 failures = pre-existing doc_profiles::manual + context_builder_token_budget, predate F011), ALL 25 integration targets GREEN incl. one_ask_acceptance 3/3, one_ask_knn_parity 2/2, constitution_v1 5/5, lgd gates. [US1] GREEN: `cargo test --lib drafting` (70+), `cargo test --test one_ask_acceptance` (3), `cargo test --lib query_router` (50+) all pass on PG via `test_pg()`; T011's RED list cleared.
- [X] T018 ✅ 2026-06-12 PASSED — research KNN 2.0 s (≤15 s gate; was 1678 s), 0 oomd, draft completed end-to-end; fresh index 10 docs/20.8k chunks = 278 MB (vs 65 GB). Env: pg root relocated to ext4 (NTFS unsupported), oomd drop-in applied, engine password persistence fixed. Draft QUALITY issues recorded → 010 writer pipeline. [US1] Live dev gate: `npm run tauri dev`, fresh-index the 8-PDF Sindh corpus, run one one-ask draft end-to-end; verify each research lookup ≤ 15 s in `logs/dev-*.log` (sqlx timings) and `journalctl` shows no oomd action. ALSO note per-document indexing time on this fresh index vs the current-system baseline — an early SC-006 probe so a gross HNSW-insert regression surfaces BEFORE the 65 GB migration (full ±10% check stays at T035). (User runs/observes per workflow; SC-001 + first SC-002 data point. Full 65 GB-scale check follows US3 migration.)

**Checkpoint**: MVP — drafting usable on real corpus in dev.

---

## Phase 4: User Story 2 — One installer, zero DB setup (P2)

**Goal**: engine is an invisible, robust internal component; packaging carries everything.

**Independent Test**: clean machine (or fresh user + empty app_data dir), offline: install/launch → "ready to index" with zero manual steps; kill -9 the app → relaunch recovers.

- [ ] T019 [P] [US2] Orphan handling in `src-tauri/src/db/pg/engine.rs`: postmaster.pid + `pg-instance.json` version check → adopt healthy orphan, else stop+restart; crash recovery = PG WAL replay surfaced as normal startup (FR-005). Unit tests in-module with temp dirs.
- [ ] T020 [P] [US2] Port-conflict auto-retry (new free port, persist to pg-instance.json) + test that a foreign PG on the default port is never connected to (edge case).
- [ ] T021 [US2] Startup error UX: map every `EngineError` variant → plain-language message + one remediation, surfaced in frontend startup screen `src/components/StartupError.tsx` (new) wired in `src/App.tsx` (FR-012); no silent hang path.
- [ ] T022 [US2] Packaging: bundle PG archive + vendored pgvector into the Tauri installer (`src-tauri/tauri.conf.json` resources/sidecar config); verify `npm run tauri build` artifact contains them and first-run works from the built bundle, offline.
- [ ] T023 [US2] Loopback enforcement check in new `src-tauri/tests/pg_lifecycle.rs` (pg_spike.rs stays frozen as the T001 gate artifact): connection attempt from non-loopback address refused (FR-006); clean shutdown on app exit verified (no orphan after normal quit).

**Checkpoint**: SC-003 demonstrable on a clean machine.

---

## Phase 5: User Story 3 — ~~Corpus migration~~ CANCELLED 2026-06-12 (user decision: fresh re-index, no migration; old echo.db deleted after US1 live gate passes)

**Goal**: one-time echo.db → PG transfer; bit-faithful vectors; resume-safe.

**Independent Test**: fixture SQLite DB → migrate → counts match, vectors byte-identical, re-run after simulated interrupt is idempotent.

### Tests (RED first)

- [X] T024 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [P] [US3] `src-tauri/tests/migration_verify.rs` on a small fixture echo.db (built in-test): per-table row counts match, N random vectors byte-compared, interrupt-then-resume produces no dupes, preflight refuses on (mocked) low disk + wrong schema_version — RED before T026.

### Implementation

- [X] T025 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] Additive PG migration `src-tauri/src/db/pg/migrations/002_migration_job.sql`: `migration_job` table per data-model.md (phase, table_progress JSONB, report JSONB).
- [X] T026 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] Migration service `src-tauri/src/services/migration/sqlite_import.rs`: rusqlite read-only open (rusqlite dep confined to this module), preflight (schema_version == expected, free disk ≥ 1.2× source), per-table streaming via binary COPY (`copy_in_raw`) batched, resume markers (skip done tables, truncate+recopy in-flight), HNSW index dropped before vector COPY and rebuilt after with raised `maintenance_work_mem`, verification (counts + vector spot-check) → report.
- [X] T027 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] Tauri commands `src-tauri/src/commands/migration.rs` per contract C4 (preflight/start/cancel/report + `migration://progress` events, camelCase serde) + TS API `src/api/migration.ts`.
- [X] T028 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] Migration UI `src/components/settings/MigrationPanel.tsx`: preflight result, progress (phase/table/percent), final per-table report, "old DB kept untouched" messaging; wire into Settings.
- [X] T029 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] GREEN: T024 passes; `npx tsc --noEmit` clean.
- [X] T029a ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] Capture parity baseline from the OLD database BEFORE cutover: run the standing eval prompt set's research lookups against the live echo.db (exact cosine path) and record top-K results to `specs/011-postgres-pgvector-port/parity_baseline.json`. **MUST complete before T030 — there is no second chance once migration cuts over.**
- [X] T030 ❌ CANCELLED 2026-06-12 (user: wipe old 65 GB echo.db, re-index fresh on PG — no data migration) [US3] Live migration of the real 65 GB echo.db (user-attended; AC + systemd-inhibit per standing practice; oomd drop-in applied first; T029a baseline captured): record duration vs SC-004 ≤ 4 h, verify report 100%, then re-run T018's live gate on the FULL corpus (completes SC-001/SC-002 at true scale).

**Checkpoint**: production corpus on PG; old echo.db retained until user confirms.

---

## Phase 6: User Story 4 — One storage contract + governance (P4)

**Goal**: parity proven, seam enforced, constitution amended — merge gate cleared.

**Independent Test**: parity suite + seam audit + full test sweep all green; constitution v2.0.0 ratified.

- [ ] T031 [P] [US4] Parity test `src-tauri/tests/pg_parity.rs`: standing eval prompt set, top-K overlap ≥ 95% — HNSW result vs EXACT scan on the SAME fresh PG corpus (run query with index vs `SET LOCAL enable_indexscan = off`); old-DB baseline obsolete (migration cancelled); raise `hnsw.ef_search` if < 95% (SC-005). Drafted citations still pass verbatim verify gate (reuse one_ask_acceptance malicious-writer fixture).
- [ ] T032 [P] [US4] Seam audit script/test: grep-based check that `vector`/`<=>`/`halfvec` SQL appears only in `retrieval.rs` + migrations, `tsquery|tsvector|ts_rank` only in `query_router.rs` + migrations (FR-009); wire as `src-tauri/tests/seam_audit.rs` (read-source test).
- [ ] T033 [US4] Full sweep (SC-007): all existing suites against `test_pg()` — lib + integration + acceptance gates (`constitution_v1_acceptance`, `lgd_drafting_acceptance`, `lgd_ingest_gate`, `one_ask_acceptance`) with no weakened assertions; fix stragglers.
- [ ] T034 [US4] Constitution amendment v2.0.0 in `.specify/memory/constitution.md`: §IV → "single app-managed local store (bundled PostgreSQL + pgvector); no user-installed or remote database processes"; stack-invariant rows (vector/FTS/DB) updated; Sync Impact Report header; references ADR-0015. **MERGE GATE: master merge blocked until this lands (FR-013).**

**Checkpoint**: cutover eligible for master.

---

## Phase 7: Polish & Cross-Cutting

- [ ] T035 [P] Indexing throughput check (SC-006): index one large PDF pre/post port on same hardware; within ±10% (HNSW insert cost vs removed vec0/FTS5 upserts) — tune if regressed.
- [ ] T036 Draft-stability gate (SC-002): 10 consecutive one-ask drafts on full corpus without oomd action; retire/simplify `run-dev.sh` oomd scope wrapper if no longer needed.
- [ ] T037 [P] Dead-code cleanup: remove sqlite-vec loading, FTS5 schema remnants, unused sqlite deps from `src-tauri/Cargo.toml` (rusqlite stays, confined to migration module); update `quickstart.md` with measured numbers.
- [ ] T038 CHANGELOG_DEV.md entry (symptom/fix/impact) + memory update; run quickstart.md validation end-to-end.

---

## Dependencies & Execution Order

- **Phase 1 → Phase 2 → Phase 3 (US1)** are strictly sequential (port nature: nothing compiles against PG until T010/T011).
- **US2 (Phase 4)** depends on Phase 2 only — can run in parallel with US1 after T011 (different files: engine.rs/packaging vs retrieval/query_router).
- **US3 (Phase 5)** CANCELLED 2026-06-12 — fresh re-index replaces migration; old echo.db deleted (user runs the rm) after T018 live gate passes.
- **US4 (Phase 6)** T031 self-contained (HNSW vs exact scan on PG); T033 depends on US1; T034 independent (can draft anytime, must land before merge).
- **Phase 7** after all desired stories.

### Parallel opportunities

- T003 ∥ T001/T002; T005, T009 ∥ T004/T006; T012 ∥ T013a; T019 ∥ T020; T024 early; T031 ∥ T032; T035 ∥ T037.
- After T011: one stream on US1 (T012–T018), a second on US2 (T019–T023).

---

## Implementation Strategy

**MVP = Phase 1 + 2 + US1** (drafting usable in dev on a fresh-indexed corpus). Then US2
(shippable installer), US3 (carry the 65 GB corpus), US4 (parity + governance = merge
gate), Polish. Stop-and-validate at every checkpoint; commit per task group. The T001
spike is the only task allowed to invalidate the plan — if offline pgvector fails,
re-open research R2 before writing any port code.

**Totals**: 40 tasks — Setup 3, Foundational 8, US1 8 (incl. T013a), US2 5, US3 8 (incl. T029a), US4 4, Polish 4.
