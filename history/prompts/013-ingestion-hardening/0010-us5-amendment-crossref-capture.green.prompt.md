---
id: 0010
title: Phase 7 — US5 amendment + cross-reference capture (FR-014/15)
stage: green
date: 2026-06-20
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: engr.riaz2010@gmail.com
command: "go ahead with phase 7"
labels: [ingestion, graph, amendment-events, cross-reference, US5]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/legal_ir/amendment_parser.rs
  - src-tauri/src/services/structured_importer.rs
  - specs/013-ingestion-hardening/tasks.md
  - MODULAR_IMPROVEMENT_LOG.md
tests:
  - "cargo test --lib: 510 passed / 0 failed (+6: amendment_parser ×5, anchor ×1)"
---

## Prompt

go ahead with phase 7

## Response snapshot

Found cross_reference edges already built in graph_builder; the gap was amendment
events. Implemented deterministic `parse_amendment` in the empty `legal_ir/
amendment_parser.rs` stub (kind + verbatim by_instrument + w.e.f date; None for
non-amendment footnotes). Wired `persist_amendment_events` into structured_importer:
parse each isolated footnote, anchor to the single distinct Article on its page
(ambiguous/absent → skip, never invent), insert amendment_events row. No
amended_by edge (instrument isn't a graph node). Full lib 510/0.

Also answered the user's architecture question: hierarchical chunking = the content
store (tree → chunks); graph = the relationship store (same tree as nodes + edges
contains/cross_reference/amended_by); joined by graph_node.chunk_id → chunks.id;
graph powers retrieval-time expansion + structural questions.

## Outcome

- ✅ Impact: amendment history captured verbatim into amendment_events; cross-refs
  confirmed; relationships now in the graph store (capture only, no query change).
- 🧪 Tests: full lib 510 pass / 0 fail (+6).
- 📁 Files: amendment_parser.rs (parser+tests), structured_importer.rs (wiring+anchor+test),
  tasks.md, MODULAR_IMPROVEMENT_LOG.md.
- 🔁 Next prompts: Phase 8 acceptance harness + SC gate; cumulative live re-index.
- 🧠 Reflection: the schema + cross_reference were already there; the real work was a
  small verbatim parser + a conservative anchor that refuses rather than guesses.

## Evaluation notes (flywheel)

- Failure modes observed: conservative one-Article-per-page anchor is sparse on
  multi-article pages (safe, but captures few events) — follow-up = marker-position anchor.
- Graders run and results (PASS/FAIL): full lib gate PASS (510/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): re-index; count amendment_events rows +
  cross_reference edges; sample by_instrument for verbatim fidelity.
