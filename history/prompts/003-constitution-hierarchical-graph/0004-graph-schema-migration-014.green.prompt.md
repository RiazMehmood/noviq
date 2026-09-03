---
id: 0004
title: graph schema migration 014
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (implementation — tasks T001, T004)
labels: [legal-rag, graph, sqlite, migration, schema, constitution]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: N/A
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: N/A
files:
  - src-tauri/src/db/migrations/013_hierarchical_chunks.sql
  - src-tauri/src/db/migrations/014_constitution_graph.sql
  - src-tauri/src/db/schema.rs
  - src-tauri/src/services/doc_profiles/ (new dir)
  - tools/constitution_import/src/ (new dir)
  - specs/003-constitution-hierarchical-graph/tasks.md
  - specs/003-constitution-hierarchical-graph/data-model.md
  - CHANGELOG_DEV.md
tests:
  - T005 schema verification — PENDING (needs cold build + launch)
---

## Prompt

`yes` — proceed with implementation. Began executing tasks.md for feature
003-constitution-hierarchical-graph, starting at the foundational tasks.

## Response snapshot

Read `CHANGELOG_DEV.md` (mandatory), `schema.rs`, and the migrations directory
first — which surfaced three things the plan missed:

1. **Plan drift**: migrations are SQL files in `src-tauri/src/db/migrations/`
   (run via `sqlx::migrate!`), not Rust in `schema.rs`. Corrected tasks.md +
   data-model.md.
2. **CHECK-constraint wrinkle**: `chunks.unit_type` has a `CHECK IN
   ('document','section','leaf')`; SQLite cannot ALTER a CHECK. User chose
   (AskUserQuestion) to **edit migration 013 in place** to widen the list —
   safe because a schema bump nuclear-wipes echo.db and replays all migrations.
3. **Destructive side-effect**: bumping `TARGET_SCHEMA_VERSION` 13→14 wipes the
   live echo.db + `.fastembed_cache`. User consented.

Implemented (T001, T004):
- Widened the `unit_type` CHECK in `013_hierarchical_chunks.sql` to add
  part/chapter/article/clause/subclause (kept section/leaf).
- New `014_constitution_graph.sql` — `graph_nodes` + `graph_edges` with
  composite edge indexes; sets `schema_version=14`.
- Bumped `TARGET_SCHEMA_VERSION` 13→14 in `schema.rs`.
- Created the directory skeleton (`doc_profiles/`, `tools/constitution_import/`).
- CHANGELOG_DEV.md entry; tasks.md T001/T004 checked.

## Outcome

- ✅ Impact: foundational graph schema in place; S3–S7 unblocked once verified.
- 🧪 Tests: T005 verification PENDING — requires a cold `cargo build` + launch
  (the user's ~30-min step on this NTFS/bfd-linker setup).
- 📁 Files: 2 migrations, schema.rs, 2 new dirs, 3 docs updated.
- 🔁 Next prompts: user runs the T005 build/launch + schema assertions; then
  T002 (Cargo workspace binary for the importer) → S1.
- 🧠 Reflection: reading the actual migration files before coding caught a
  plan error (Rust vs SQL) and a hard SQLite limitation (CHECK immutability)
  that would have failed at build/insert time. Verify mechanism, don't assume.

## Evaluation notes (flywheel)

- Failure modes observed: plan.md described the migration mechanism from memory
  incorrectly — corrected against the real codebase.
- Graders run and results (PASS/FAIL): none yet — T005 is the schema grader,
  pending build.
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): user runs the T005 cold build +
  launch; confirm `schema_version=14` and the two graph tables exist.
