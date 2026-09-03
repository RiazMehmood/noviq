---
id: 0027
title: S7 — Constitution v1 acceptance harness (A1-A4) PASS
stage: green
date: 2026-05-24
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (option 3, move on to S7)
labels: [s7, acceptance, harness, A1, A2, A3, A4, ci-gate, v1-close]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/tests/constitution_v1_acceptance.rs
    change: NEW — ~270 lines, 5 tests covering A1 structural / A2 citation accuracy / A3 graph integrity / A4 citation-fast-path subset (F1/F2/F4 + bonus 19A) + A4 Preamble. In-memory SqlitePool, no LLM, runs in 1.6s. Preamble exempted from Article-label check (Idea 1 parent + paragraph children carry their own invariants).
  - path: src-tauri/tests/structure_parser_test.rs
    change: Drift fix — Article=312→313 (Preamble import-gap fix added one article-level node), Clause=787→808 (Idea 1 paragraph splitting added 21 clause-level children). Without this the existing test would break next re-import.
  - path: /home/riaz/Desktop/db_inspection_constitute_v1/09_citation_accuracy.sh
    change: Added the same `unit_label == 'Preamble'` exemption as the Rust harness, so the bash probe agrees. Pre-patch reported 20 false-positive FAILs (the 20 Preamble chunks); post-patch checked=1426 failed=0.
  - path: specs/003-constitution-hierarchical-graph/T030-acceptance-results.md
    change: NEW — full T030 record (Rust harness + 3 bash probes + cross-cutting observations + v2 follow-up list).
  - path: CHANGELOG_DEV.md
    change: 2026-05-24 entry combining T029/T030/T031 — symptom/why/fix/files/cross-cutting/v2-followups.
tests:
  - name: a1_structural_counts_and_linkage
    type: integration
    result: NEW — 1 doc · 12 parts · 28-32 chapters · 313 articles · 808 clauses · ≥300 subclauses · orphans=0 · max-children ratio <0.90
  - name: a2_citation_accuracy_every_clause_aligned_with_article
    type: integration
    result: NEW — every article/clause/subclause's unit_label, unit_path, citation, content lead-in, parent-chain article all agree
  - name: a3_graph_integrity_nodes_edges_endpoints
    type: integration
    result: NEW — graph_nodes=chunks, no dangling, contains=N-1, cross_reference>50, non_obstante>0
  - name: a4_citation_fastpath_resolves_f1_f2_f4_seeds
    type: integration
    result: NEW — F1/F2/F4 + Article 19A bonus, fast-path head label matches expected
  - name: a4_preamble_fastpath_returns_parent_plus_children
    type: integration
    result: NEW — "what does the preamble say" returns ≥2 chunks (parent + paragraph children)
  - name: parse_tree_builds_six_level_hierarchy (existing, updated)
    type: integration
    result: UPDATED — Article assertion 312→313, Clause 787→808
---

## Prompt

> option 3, move on to S7

## Response snapshot

User chose option 3 from the NER quality follow-up (park the issue, ship S7). S7 = T029-T031 = automate the A1-A4 gate as a CI-runnable test, run it end-to-end including the bash db_inspection scripts, and write the closing CHANGELOG entry.

**Implementation:**
1. Read spec.md to confirm A1-A4 wording, tasks.md for T029-T031 scope, existing bash db_inspection scripts (07/09/10) to source the assertion logic.
2. Fixed drift in tests/structure_parser_test.rs — Article and Clause assertions hadn't been updated since the Preamble import-gap fix and Idea 1; would have failed next time they ran. Bumped to 313 and 808 with explanatory comments.
3. Wrote tests/constitution_v1_acceptance.rs from scratch — 5 tests, in-memory SqlitePool, no LLM, deterministic. A1 covers counts + linkage + orphans + max-children ratio (the "no parent holding >90% of leaves" spec rule). A2 mirrors db_inspection/09's logic in Rust, with Preamble exempted (Idea 1 parents + paragraph children don't carry Article N labels). A3 covers graph_nodes count + no dangling endpoints + contains = N-1 + cross_reference and non_obstante present. A4 covers the citation-fast-path subset (F1/F2/F4 + Article 19A bonus) and a Preamble parent-plus-children check; F3/F5 explicitly out-of-scope per spec.
4. Ran the harness — 5/5 PASS in 1.99s on first try.
5. Ran T030 full gate (harness + bash 07/09/10). Bash 09 reported 20 false-positive FAILs (Preamble rows missing the Article label). Patched the bash script with the same exemption — re-ran clean (checked=1426 failed=0).
6. Wrote T030-acceptance-results.md capturing every output verbatim.
7. Wrote 2026-05-24 CHANGELOG entry combining T029+T030+T031 — symptom/why/fix/cross-cutting/v2-followups.

## Outcome

- ✅ Impact: v1 acceptance gate closed. Any future change to the constitution pipeline can be CI-gated on `cargo test --test constitution_v1_acceptance` — protects against silent regressions to A1/A2/A3/A4 invariants. The bash db_inspection scripts now agree with the harness for parity. v2 unblocked.
- 🧪 Tests: 5 new integration tests, all passing; 1 existing test fixed for Idea-1 drift.
- 📁 Files: 1 new harness (~270 lines), 1 fixed test, 1 patched bash script, 1 new results doc (~140 lines), 1 CHANGELOG entry (~85 lines).
- 🔁 Next prompts: (a) T032-T034 polish (docs as-built notes, quickstart end-to-end, surface ADR — feature 003 Phase 10). (b) v2 work — OCR/extraction for govt corpus.
- 🧠 Reflection: Encoding the bash probes as a Rust harness was the right move — it surfaced one immediate drift (Article=312 was stale) and provides a permanent regression net. The Preamble-exemption decision was the only judgement call; documented inline in both the Rust test and the bash script so future readers see the same rationale.

## Evaluation notes (flywheel)

- Failure modes observed: 20 false-positive FAILs from bash 09 because the script predated Idea 1's Preamble shape. Caught immediately on first T030 run; patched in 6 lines. Pattern: when you add a new structural concept (Preamble exempt from Article label), every probe that checks "is it an Article?" needs the same exemption. Cheap to fix; cheaper to catch early.
- Graders run and results (PASS/FAIL): all 5 new + 1 fixed test PASS; bash 07/09/10 PASS post-fix.
- Prompt variant (if applicable): n/a
- Next experiment: run the harness in CI on every PR touching `services/structure_parser.rs`, `services/graph_builder.rs`, `db/migrations/`, or `services/query_router.rs::citation_fast_path`.
