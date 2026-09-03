---
id: 090
title: Phase 2 amendment-awareness annotation (pivot from amended_by edges)
stage: green
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: document-full-summary
branch: feat/document-full-summary
user: Riaz Mehmood
command: (conversation)
labels: [graphrag, amendment, retrieval, green, pivot]
links:
  spec:
  ticket:
  adr: history/adr/0020-graph-augmented-retrieval-on-postgres-not-neo4j.md
  pr:
files:
  - src-tauri/src/db/repositories/graph.rs
  - src-tauri/src/services/query_router.rs
  - CHANGELOG_DEV.md
tests:
  - lib graph_ (5 pass, EXIT_CODE=0 on VM)
---

## Prompt

"proceed into Phase 2" — then, after I surfaced the data finding, "pivot Phase 2 to amendment-awareness annotation".

## Response snapshot

Audited `amendment_events` (108 rows) before building: every `by_instrument` is an instrument OUTSIDE the corpus (Constitution amendment Acts, P.O. No. 14 of 1985, S&GAD gazette notifications, "Act No. IV/13") — none match our 16 docs. So an `amended_by` graph edge has **no in-corpus to_node** → zero edges. Flagged this rather than build dead machinery; user approved the pivot.

Implemented annotation instead: `graph::amendment_rows_for_chunks(pool, chunk_ids, scope_source_ids)` (scope-clamped join amendment_events→graph_nodes→chunks, FR-001). In `query_router.rs`, after `build_context`, when `ECHO_AMEND_NOTES` is on, build a per-chunk note `[⚠ Amendment history: this provision was <kind> by <instrument>, <date> — the text above may be superseded; verify the current version.]`; threaded an `amend_notes` map into `build_context_string` (both call sites, main + grounding-retry). Flag default OFF; `amended_by` kept in the Phase 0 edge set for free future activation. Committed `4f0a7c03`; VM build green, 5 graph_ tests pass.

## Outcome

- ✅ Impact: Completeness intent preserved without hollow edges — answers can now flag superseded base text.
- 🧪 Tests: lib graph_ 5 PASS, EXIT_CODE=0
- 📁 Files: graph.rs, query_router.rs, CHANGELOG_DEV.md (latest10)
- 🔁 Next prompts: Phase 3 cross-doc linker
- 🧠 Reflection: Auditing the target data first turned a planned edge-build into a cheaper, correct annotation — the right call was to change shape, not force the plan.

## Evaluation notes (flywheel)

- Failure modes observed: original plan (amended_by edges) would have shipped zero edges.
- Graders run and results (PASS/FAIL): VM build PASS; live trace later confirmed "annotated 1 of 33"
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): have the grounding system prompt surface (not cite) the note.
