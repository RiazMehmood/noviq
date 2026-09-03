---
id: 0020
title: S6 T026/T027 fast-path + graph expansion wired
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: Riaz Mehmood
command: (conversation)
labels: [constitution, retrieval, query-router, graph-expansion]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/db/repositories/graph.rs
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib -- citation — 10 passed / 0 failed (clean compile of route_scoped)
---

## Prompt

[The user chose graph-expansion gating "Option 1" (citation queries only),
after asking for a plain-language explanation of what graph expansion is.]

## Response snapshot

Explained graph expansion in plain terms (following "see also" links between
Articles) and the speed trade-off. The user picked Option 1 — expansion only on
citation queries.

Implemented T026: a citation branch in `route_scoped` between the cache check
and hybrid retrieval. When `citation_fast_path` resolves a unit,
`expand_citation_neighbours` maps the cited chunks to graph nodes
(`graph::node_ids_for_chunks`), follows `cross_reference`/`non_obstante` edges
one hop (`graph::expand`), `fetch_chunks_by_ids` loads the union, and vector
search / FTS5 / reranker are bypassed (reranker condition gained
`&& retrieval_tier != "citation"`). Graph errors are non-fatal. Ordinary
queries fall through unchanged.

T027 resolved as a no-op — the `graph` repo is free functions over
`&SqlitePool`; `route_scoped` already has the pool, so no Tauri state wiring.

`cargo test --lib` compiles clean; 10/10 query_router tests pass.

## Outcome

- ✅ Impact: S6 code complete (T025–T027) — citation queries now resolve
  deterministically and pull in cross-referenced/non-obstante Articles, with
  zero graph cost on ordinary chat. T028 (A4 eval) is the remaining manual run.
- 🧪 Tests: `cargo test --lib -- citation` 10/0; clean compile of the modified
  `route_scoped`. Route-level behaviour is exercised by T028.
- 📁 Files: query_router.rs (citation branch + 2 helpers), graph.rs
  (node_ids_for_chunks), tasks.md, CHANGELOG_DEV.md.
- 🔁 Next prompts: T028 — F1–F5 eval (`run_eval.py`) for A4; then S7
  (constitution_v1_acceptance.rs harness + full A1–A4 gate) and Polish.
- 🧠 Reflection: gating graph expansion to citation queries kept a latency-add
  off the hot path entirely — the user's chat-speed priority drove the design,
  not just the feature spec.

## Evaluation notes (flywheel)

- Failure modes observed: none — clean compile, 10/10.
- Graders run and results (PASS/FAIL): PASS (compile + query_router tests);
  A4 eval (T028) PENDING (manual).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run T028 F1–F5 eval, inspect
  whether F1/F2/F4 now resolve via the fast-path.
