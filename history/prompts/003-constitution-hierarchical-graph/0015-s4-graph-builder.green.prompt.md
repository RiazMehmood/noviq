---
id: 0015
title: S4 graph builder and bounded expand
stage: green
date: 2026-05-22
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: Riaz Mehmood
command: (conversation)
labels: [constitution, graph, edges, recursive-cte]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - src-tauri/src/db/repositories/graph.rs
  - src-tauri/src/services/graph_builder.rs
  - src-tauri/src/db/repositories/mod.rs
  - src-tauri/src/services/mod.rs
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib -- graph_builder — 3 passed / 0 failed
---

## Prompt

[After committing S3, the user said: "commit S3 then go for S4 and what is
total task count?"]

## Response snapshot

Committed S3 as `3f5aafb1`; reported the task count (34 total, 15 done at that
point).

S4: built the graph layer.
- T018 — `db/repositories/graph.rs`: `GraphNode`/`GraphEdge`, `insert_nodes`/
  `insert_edges`, and `expand` as a `WITH RECURSIVE … UNION` CTE (visited-set
  dedup), `max_depth` hard-capped at 3 per ADR-0007.
- T019 — `services/graph_builder.rs`: `build_graph(tree, &persisted_chunks)`,
  two-pass — pass 1 a node per tree node (chunk_id via `unit_label`), pass 2
  `contains` + `cross_reference` + `non_obstante` edges. Self-Article refs
  skipped. `regex_lite` (the repo's regex crate) via `OnceLock`.
- T020 — 3 in-crate tests.

Surfaced a real schema gap rather than inventing data: `amended_by` (in the
migration-014 `edge_type` CHECK) cannot be built — `to_node` is a NOT NULL FK
to `graph_nodes` and `node_type` admits only the six hierarchy levels, so there
is no amendment node to point at. Left `amended_by` unbuilt; documented that a
v2 schema with amendment nodes is needed.

`cargo test --lib -- graph_builder` → 3/3 green.

## Outcome

- ✅ Impact: S4 T018–T020 done — the legal-document graph builds from the
  persisted hierarchy with a bounded-expansion primitive. S5 unblocked.
- 🧪 Tests: `cargo test --lib -- graph_builder` 3/3 — one node per chunk (FKs
  resolve), `contains` = nodes−1, cross_reference + non_obstante present, 0
  dangling endpoints; `expand` depth-0 = seed, depth-1 contains from root = 13.
- 📁 Files: `graph.rs` + `graph_builder.rs` (new), 2 mod.rs registrations,
  tasks.md, CHANGELOG_DEV.md.
- 🔁 Next prompts: S5 (T022–T024) multi-level embedding — T023 wires the
  constitution path in `indexer.rs`; plus the deferred S3 T016/T017 + S4 T021
  db_inspection acceptance, unblocked once T023 populates `echo.db`.
- 🧠 Reflection: when a task (`amended_by`) cannot be done as written because
  the schema forbids it, the honest move is to build what fits, not fabricate a
  target — surface the gap with the precise constraint and a v2 path.

## Evaluation notes (flywheel)

- Failure modes observed: none — clean compile, 3/3.
- Graders run and results (PASS/FAIL): PASS (`graph_builder` lib tests 3/3).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): S5 T022 — extend `chunk_tree` for
  sub-clause/article/part chunk emission.
