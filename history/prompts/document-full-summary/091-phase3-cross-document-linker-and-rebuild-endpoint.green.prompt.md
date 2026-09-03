---
id: 091
title: Phase 3 cross-document linker + rebuild_graph endpoint
stage: green
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: document-full-summary
branch: feat/document-full-summary
user: Riaz Mehmood
command: (conversation)
labels: [graphrag, cross-document, linker, endpoint, green]
links:
  spec:
  ticket:
  adr: history/adr/0020-graph-augmented-retrieval-on-postgres-not-neo4j.md
  pr:
files:
  - src-tauri/src/services/graph_linker.rs
  - src-tauri/src/services/graph_builder.rs
  - src-tauri/src/services/mod.rs
  - src-tauri/src/db/repositories/graph.rs
  - src-tauri/src/api/system.rs
  - src-tauri/src/api/router.rs
  - CHANGELOG_DEV.md
tests:
  - graph_linker::tests (3 pass) + lib graph_ (17 pass, EXIT_CODE=0)
---

## Prompt

Confirmed three design points (title from file_name; bare "the Act" deferred; admin endpoint not per-doc auto), "build Phase 3". Then "build the rebuild_graph endpoint".

## Response snapshot

Grounded first: audited cross-document title mentions — Compendium names "Local Government Act" 24×, SPPRA "Public Procurement Act" 3×, Delegation 1×, RoB 2×; those Acts (SLGA 2013, SPP Act 2009) are in corpus; "Civil Servants Act" base is absent → correctly skipped under no-fuzzy. Confirmed the value is Rules→enabling-Act.

`services/graph_linker.rs::link_corpus`: post-ingest pass building a target registry (title regex via `title_signature` + document node + provision-head index), scanning each source's chunk text for other instruments' titles, resolving (b) provision named after the title → that node, else (a) target document node; inserts cross-doc `cross_reference` (Phase 0's expand already follows it). `title_signature` needs a type word + ≥2 core tokens (else None — too generic), `\W+` absorbs punctuation. `graph::delete_cross_doc_cross_reference_edges` = idempotent. Admin `POST /api/link_graph`. Committed `7a82fa7a`.

rebuild_graph: `rebuild_overlay` regenerates the within-doc overlay from existing graph_nodes + chunks.content (same extraction, reusing `provision_refs`; `delete_intra_doc_overlay_edges` idempotent). Admin `POST /api/rebuild_graph` = rebuild_overlay(intra) then link_corpus(cross), inline, no re-OCR/re-embed. Exposed graph_builder helpers `pub(crate)`. Committed `ad0e80dd`. VM: 17 graph_ tests pass (incl. 3 new title_signature tests), EXIT_CODE=0.

## Outcome

- ✅ Impact: Cross-doc overlay + a no-re-ingest path to regenerate the whole graph from existing data.
- 🧪 Tests: 17 graph_ PASS, EXIT_CODE=0
- 📁 Files: new graph_linker.rs + graph repo/api wiring; CHANGELOG_DEV latest11+latest12
- 🔁 Next prompts: deploy + rollout (4 steps)
- 🧠 Reflection: Cross-doc `cross_reference` reuses the Phase 0 traversal for free — no new retrieval code; the rebuild endpoint made a full re-index unnecessary.

## Evaluation notes (flywheel)

- Failure modes observed: none; UTF-8 boundary snap added for the post-title window.
- Graders run and results (PASS/FAIL): VM `cargo test --lib graph_` PASS
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): support bare "the Act" self-reference resolution (v2).
