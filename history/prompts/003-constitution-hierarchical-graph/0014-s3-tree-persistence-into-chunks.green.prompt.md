---
id: 0014
title: S3 tree persistence into chunks
stage: green
date: 2026-05-22
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: Riaz Mehmood
command: (conversation)
labels: [constitution, persistence, chunks, hierarchy]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - src-tauri/src/db/repositories/chunks.rs
  - src-tauri/src/services/structure_parser.rs
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib -- persist chunks — 3 passed / 0 failed
---

## Prompt

[After committing Feature 003 onto the new `003-constitution-hierarchical-graph`
branch, the user said "go for next task" — i.e. begin S3 — adding "you are
consuming too much token so care for that."]

## Response snapshot

Committed Feature 003 as `c70cc4a7` on a new `003-constitution-hierarchical-
graph` branch (staged 003 files explicitly — no `echo.db`, eval logs or caches;
non-003 reranker/indexer edits left uncommitted to follow the tier3 branch).

For S3, scoped to the verifiable core given the token-budget note: T014 + T015
+ a unit test, deferring T016/T017 (db_inspection script + acceptance run) which
need a populated `echo.db` and cannot be verified this session.

- T014 (`chunks.rs`): added `bulk_insert_with_ids` — single-transaction insert
  with caller-assigned IDs in the given order; `bulk_insert` delegates to it.
  Needed so a tree child's `parent_chunk_id` references a parent ID fixed
  before insert.
- T015 (`structure_parser.rs`): `NodeLevel::unit_type()` → canonical storage
  vocab; `persist_tree` + pre-order `collect_chunks` flatten the `DocNode` tree
  into `(uuid, NewChunk)` rows; `NodeMetadata` is the `domain_metadata` JSON
  (citation/level/title/attached/footnote_refs — `legal_domain` deferred to S5).
- 2 `#[tokio::test]` tests added (`persist_tests` module).

`cargo test --lib -- persist chunks` → 3/3 green (one filter-arg gotcha: cargo
test takes a single TESTNAME, so multiple filters must go after `--`).

## Outcome

- ✅ Impact: S3 T014–T015 done — the L0–L5 hierarchy now persists into `chunks`
  with parent links, canonical `unit_path`, and `domain_metadata`. S4 unblocked.
- 🧪 Tests: `cargo test --lib -- persist chunks` 3/3 — 1 document root / 0
  orphans / 312 article chunks; `Article 8(3)(b)` `unit_path` + citation
  canonical; refactored `bulk_insert` test still green. S2 test still 3/3.
- 📁 Files: `chunks.rs` (+`bulk_insert_with_ids`), `structure_parser.rs`
  (+`unit_type`/`persist_tree`/`collect_chunks`/`NodeMetadata`/tests), tasks.md,
  CHANGELOG_DEV.md.
- 🔁 Next prompts: S4 (T018–T021) graph builder; plus deferred S3 T016/T017
  (db_inspection `09` + acceptance) once persistence is wired into indexing.
- 🧠 Reflection: under a token-budget constraint, scoping a stage to its
  verifiable core (impl + unit test) and explicitly deferring the parts that
  need a populated DB keeps the increment honest without burning tokens on an
  un-runnable acceptance gate.

## Evaluation notes (flywheel)

- Failure modes observed: `cargo test --lib a b` rejects the 2nd filter — must
  be `cargo test --lib -- a b`. One wasted build cycle.
- Graders run and results (PASS/FAIL): PASS (`persist` + `chunks` lib tests
  3/3; S2 integration 3/3).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): S4 T018 — `graph.rs` repo with the
  bounded recursive-CTE `expand`.
