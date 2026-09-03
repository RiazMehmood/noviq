---
id: 0016
title: S5 multi-level embedding and indexer wiring
stage: green
date: 2026-05-22
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: Riaz Mehmood
command: (conversation)
labels: [constitution, indexer, embedding, doc-type-routing]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/indexer.rs
  - src-tauri/src/db/repositories/graph.rs
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib -- persist graph_builder — 6 passed / 0 failed (+ clean compile of indexer.rs)
---

## Prompt

[The user said "continue into S5". Mid-stage they asked for a plain-language
explanation of "IR" and why only the constitution, noting they will later
upload Acts/Rules/Policies; then chose the doc-type-routing approach for T023.]

## Response snapshot

Explained that IR = the clean structured JSON the S1 importer produces, and
that v1 is constitution-only by design but the `DocProfile` abstraction already
generalizes to Acts/Rules. The user picked Option 1 (detect doc-type in the
indexer) — it scales to the future govt corpus with one unified "Index" flow.

S5:
- T022 — multi-level chunk content. A bare sub-clause is context-blind, so
  `node_content` in `structure_parser.rs` prepends the Article heading + clause
  chapeau to each leaf's `content`; a `context` arg threads through
  `collect_chunks`. Implemented in `structure_parser`/`persist_tree`, NOT
  `chunker.rs::chunk_tree` as tasks.md named — `chunk_tree` is for the
  heading-detector tree, off the constitution path.
- T023 — indexer wiring. `try_load_constitution` (doc-type routing hook),
  `index_constitution` (parse → delete_for_source → persist → build_graph →
  insert → embed → finalize), `do_index` routes to it. The embed/upsert block
  was extracted to a shared `embed_and_upsert`; added `graph::delete_for_source`
  for clean re-index.

`cargo test --lib` compiles clean (only pre-existing `scheduler.rs` warnings);
6/6 regression tests pass. T024 (the db_inspection acceptance) is a manual app
run — flagged, not done.

## Outcome

- ✅ Impact: S5 T022–T023 done — the constitution indexes end-to-end through
  the structure-aware path; leaf chunks embed with ancestor context. T024 is a
  manual run; it also unblocks the deferred T016/T017/T021.
- 🧪 Tests: `cargo test --lib -- persist graph_builder` 6/6; indexer.rs +
  graph.rs compile clean. `index_constitution` has no unit test (needs the
  ONNX embedder) — its verification is T024.
- 📁 Files: `structure_parser.rs` (node_content/context), `indexer.rs`
  (try_load_constitution/index_constitution/embed_and_upsert + routing),
  `graph.rs` (delete_for_source), tasks.md, CHANGELOG_DEV.md.
- 🔁 Next prompts: T024 manual acceptance (`./run-dev.sh` → index the IR →
  db_inspection 01–04); then S6 (T025–T029) retrieval pipeline.
- 🧠 Reflection: when a task names a file that the implemented architecture has
  since bypassed (`chunk_tree`), implement the task's *intent* in the right
  place and record the deviation — don't force the stale path.

## Evaluation notes (flywheel)

- Failure modes observed: none — clean compile, 6/6.
- Graders run and results (PASS/FAIL): PASS (compile + regression 6/6);
  T024 acceptance PENDING (manual).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run T024 end-to-end, then S6 T025.
