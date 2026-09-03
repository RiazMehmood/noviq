---
id: 089
title: Phase 1 generalized edge extraction + Phase 0 seed-and-expand arm
stage: green
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: document-full-summary
branch: feat/document-full-summary
user: Riaz Mehmood
command: (conversation)
labels: [graphrag, graph-builder, retrieval, green]
links:
  spec:
  ticket:
  adr: history/adr/0020-graph-augmented-retrieval-on-postgres-not-neo4j.md
  pr:
files:
  - src-tauri/src/services/graph_builder.rs
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
tests:
  - graph_builder::tests (5 pass on VM, cuda/llm/server)
---

## Prompt

"yes, (b)+(a) no fuzzy, order 1→0→2→3 … smooth the graph extraction and do needful." Then "git route, commit and push."

## Response snapshot

Phase 1 (`graph_builder.rs`): replaced `article_ref_re` with `provision_ref_re` (article|section|rule|regulation|clause|chapter|part|schedule + arabic number; arabic-only v1 to avoid false roman edges). Added `provision_key` / `citation_key` so references and stored citations normalize to one lookup key (`Rule 5(6)`/`Rules 3` → `rule5`/`rule3`), resolved against a provision-head index; self-ref/self-loop guards kept. Runs on all three ingest paths.

Phase 0 (`query_router.rs`): new flag-gated seed-and-expand arm after the hybrid merge (mirrors the summary-recall arm) — top-`ECHO_GRAPH_SEED_K` chunks → `graph::node_ids_for_chunks` → `graph::expand` over cross_reference/non_obstante/amended_by at `ECHO_GRAPH_HOPS` → scope-clamped union-merge → tier `hybrid+graph`; gated `ECHO_GRAPH_EXPAND` (default OFF).

Committed `0a6987fd`, pushed feature branch (no release deploy). Built on VM (no local Rust toolchain): 5 graph_builder tests pass. Read-only real-data proof: E&D 1973 = 135 chunks reference a Rule N, 14 distinct rules, **12 resolve to real heads** (vs 0 edges today); SPPRA 498 referencing chunks.

## Outcome

- ✅ Impact: Every document can now gain a within-doc overlay; the retrieval arm is wired and compiles.
- 🧪 Tests: 5 graph_builder tests PASS (incl. new provision_refs/key-normalize/synthetic Rule 4→Rule 12 edge)
- 📁 Files: graph_builder.rs, query_router.rs, CHANGELOG_DEV.md (latest9)
- 🔁 Next prompts: Phase 2 amendment awareness
- 🧠 Reflection: Normalizing both sides to a key (not a hard-coded target format) is what made it doc-agnostic; the synthetic Rule→Rule test locks the regression.

## Evaluation notes (flywheel)

- Failure modes observed: none in build; roman-numbered Parts/Schedules deferred by design.
- Graders run and results (PASS/FAIL): VM `cargo test --lib graph_builder` PASS; edge-yield SQL PASS
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): regenerate edges and confirm E&D 0→12+ live.
