---
id: 0006
title: Cutover compile-green and full suite green
stage: green
date: 2026-06-12
surface: agent
model: claude-fable-5
feature: 011-postgres-pgvector-port
branch: 011-postgres-pgvector-port
user: riaz
command: implement (resume from WIP 149c284b)
labels: [postgres, pgvector, cutover, test-port, halfvec, T011-T017]
links:
  spec: specs/011-postgres-pgvector-port/spec.md
  ticket: null
  adr: history/adr/0015-bundled-postgresql-pgvector-engine.md
  pr: null
files:
  - .cargo/config.toml
  - CHANGELOG_DEV.md
  - specs/011-postgres-pgvector-port/tasks.md
  - src-tauri/src/lib.rs
  - src-tauri/src/commands/search.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/src/services/reranker.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/graph_builder.rs
  - src-tauri/src/services/structured_importer.rs
  - src-tauri/src/services/doc_ir/progress_store.rs
  - src-tauri/src/services/doc_profiles/rules.rs
  - src-tauri/src/db/repositories/index_jobs.rs
  - src-tauri/src/db/repositories/chunks.rs
  - src-tauri/src/db/repositories/meetings.rs
  - src-tauri/src/db/repositories/ir_cache.rs
  - src-tauri/tests/ (10 files ported; schema_wipe_test.rs deleted)
tests:
  - cargo test --lib → 368/370 (2 pre-existing failures predate F011)
  - cargo test --tests --no-fail-fast → all 25 integration targets GREEN
---

## Prompt

recal memory and start working, and dont copy previous db data, wipe previous data so we will indexed new documents on new db.

(follow-ups in-session: "why there are too much tests? again and again can we not run a single test after completion of all tasks?" · "how many total tasks we have? how much we completed our work?" · "if all green, commit the work" · "what are the results? commit if green")

## Response snapshot

Resumed F011 at T011. Fixed 4 lib compile errors (sed-corrupted `{:?}` format
strings; 2 leftover `sqlx::Transaction<Sqlite>`). Ported all 13 in-crate test
modules + 10 integration test files from `sqlite::memory:` to the shared
bundled-PG `db::testing::test_pool()`; deleted obsolete `schema_wipe_test.rs`;
rewrote `migration_020_test.rs` against information_schema. Root-caused the
52-test "vector.so version mismatch" wave to PG version drift (env-only pin →
a cargo run without it embedded PG 18.4 vs pgvector built for 17.10); moved
`POSTGRESQL_VERSION` and `ORT_DYLIB_PATH` into `.cargo/config.toml [env]`.
Fixed sweep casualties: corrupted heading regex in `doc_profiles/rules.rs`
(restored byte-identical), `LENGTH()::bigint` INT4 decode, one missed `?`
placeholder. Re-aligned 5 full-precision `<=>` queries to the halfvec-expression
HNSW index form (T016). Updated stale fixture counts from the 2026-06-04 IR
data patch (313→361 articles, 927→1024 clauses; A2 exempts synthetic
`Article IDX-*`); shared `ModelSlot` via OnceLock in test_query_router;
lengthened FTS fixture past the 150-char production filter. Per user
instruction, batched all work and ran ONE consolidated suite at the end.

## Outcome

- ✅ Impact: T002, T007–T017 complete — US1 (MVP) backend fully ported to bundled PG 17.10 + pgvector; data migration stays cancelled (fresh re-index on empty store).
- 🧪 Tests: lib 368 passed / 2 pre-existing failures; all 25 integration targets green (one_ask 3/3, knn_parity 2/2, constitution_v1 5/5, lgd gates).
- 📁 Files: see files list; CHANGELOG_DEV.md entry added; tasks.md ticked through T017.
- 🔁 Next prompts: T018 live dev gate (user-attended fresh index of Sindh corpus, ≤15 s lookups, no oomd); then US2 (T019–T023), US4 incl. constitution §IV amendment (merge gate), polish.
- 🧠 Reflection: env-var-only build pins silently drift the moment any tool invokes cargo without them — `.cargo/config.toml [env]` is the right home. Mechanical `?`→`$N` seds must exclude regex literals.

## Evaluation notes (flywheel)

- Failure modes observed: build-time version drift masquerading as extension ABI breakage; stale acceptance counts from a data patch that never got a full-suite run; once-per-process global (LlamaBackend) racing under parallel tests.
- Graders run and results (PASS/FAIL): consolidated cargo suite PASS (minus 2 documented pre-existing).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T018 live gate; capture per-document indexing time as the early SC-006 probe.
