---
id: 0022
title: Preamble import gap fix
stage: green
date: 2026-05-23
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: chat (free-form continuation of T028/A4)
labels: [constitution, retrieval, importer, bugfix, A4-followup]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: src-tauri/src/services/structure_parser.rs
    change: parse_tree now prepends a Preamble node when ir.preamble is non-empty; new build_preamble helper; persist-test assertions updated (312→313 articles, plus positive Preamble assertion)
  - path: src-tauri/src/services/query_router.rs
    change: detect_citation matches the literal token "preamble" before the Article regex and returns "Preamble"; two test additions covering detect + fast-path resolution
  - path: CHANGELOG_DEV.md
    change: appended Preamble-fix entry above the T028/A4 section (symptom, fix, tests, ops, follow-ups)
tests:
  - name: structure_parser::persist_tests::persist_tree_writes_every_node_with_resolving_parents
    type: integration
    result: updated (assertion 312→313, new Preamble row assertion)
  - name: query_router::tests::detect_citation_parses_article_and_clauses
    type: unit
    result: extended ("preamble" → Some("Preamble"))
  - name: query_router::tests::citation_fast_path_resolves_cited_units
    type: integration
    result: extended (Preamble query resolves to a single chunk)
---

## Prompt

> 1. Fix the Preamble import gap (real bug, do first):
>    - The IR import emits no retrievable chunk for the Preamble text containing "Quaid-i-Azam Mohammad Ali Jinnah".
>    - Fix in tools/constitution_import/ (importer) and/or services/structure_parser.rs + services/indexer.rs::index_constitution (persistence path) so the Preamble becomes a queryable chunk.
>    - Re-index, re-ask Fact 5 — should pass.
>  2. Then S7 (T029–T031) — src-tauri/tests/constitution_v1_acceptance.rs automated harness + full A1–A4 gate + final CHANGELOG.
>  3. Then Polish (T032–T034) — docs as-built notes, quickstart end-to-end, surface ADR.
> first discuss about fix what we are going to fix
>
> (follow-up) update tools/constitution_import/constitute_IR/constitution_ir.json add preamble inside this, i will reindex this, and you also need to aside the current db I m closing the app do all remaining work, after completion i will do the indexing only.

## Response snapshot

**Discussion phase:** Audited the four candidate code paths and located the
single drop-point — the IR JSON already carries `"preamble"` (verified by
inspecting `constitution_ir.json`: contains the full text including
"Quaid-i-Azam Mohammad Ali Jinnah"), and `services/constitution_ir.rs`
already deserializes it. The bug was strictly in `structure_parser::parse_tree`
which built the root `Document` from `ir.parts` and ignored `ir.preamble`,
so the Preamble produced no chunk → no embedding → no FTS row → no graph
node. Clarified to user that no JSON edit was needed.

**Implementation phase:**
1. Moved `echo.db` (0 bytes, already empty) to
   `echo.db.bak-preamble-fix-20260523-095359`.
2. `structure_parser::parse_tree`: changed root `children` from a direct
   `ir.parts.iter()...collect()` to a `Vec` that optionally prepends a
   Preamble node, then extends with the parts. New helper `build_preamble`
   emits a `NodeLevel::Article` leaf with `citation = "Preamble"`,
   `unit_path = "Preamble"` (top-level), `title = None` (avoids the
   "Preamble — Preamble" duplication that `node_content`'s article render
   path would otherwise emit), empty `children`/`attached`/`footnote_refs`.
   Reused `NodeLevel::Article` to keep the enum/schema untouched.
3. `query_router::detect_citation`: added a short-circuit before the
   `Article N` regex — if the lowercased query contains `"preamble"`,
   return `Some("Preamble")`. Comment justifies the ordering choice
   (Preamble wins over Article in mixed queries; intended subject).
4. Tests: bumped the `articles` count assertion 312 → 313 in the
   persist test, added a positive `unit_label = 'Preamble'` row
   assertion, added detect-citation and fast-path coverage.
5. CHANGELOG_DEV.md: appended a full entry above T028/A4 with
   symptom/why, exact fix points, tests, operational re-index steps,
   and a note that the Art 204→Art 6 leak is *not* addressed here.

## Outcome

- ✅ Impact: Preamble becomes a first-class retrievable chunk on next index — both via hybrid (FTS5 + vector) and via the citation fast-path (`"Preamble"` → single leaf). Fact 5 ("Founder of Pakistan, Quaid-i-Azam") should now have retrieval evidence; remaining failure surface narrows to the 0.5B generator.
- 🧪 Tests: updated 1 persist integration test (assertion bump + new positive row check), extended 2 query_router tests (1 unit, 1 integration). Compile-check kicked off; user will run `cargo test -p echo-ai-app` after re-indexing.
- 📁 Files: structure_parser.rs (+~25 lines logic, +~12 lines test), query_router.rs (+~8 lines logic, +~12 lines test), CHANGELOG_DEV.md (+~60 lines entry).
- 🔁 Next prompts: (a) S7 T029–T031 — `src-tauri/tests/constitution_v1_acceptance.rs` automated harness + A1–A4 gate; (b) separately, fix the Art 204→Art 6 fast-path leak that A4 surfaced.
- 🧠 Reflection: The "missing data" intuition was wrong — IR + deserializer were correct; the loss was at tree assembly. Saved real time by auditing the entire ingestion path (importer → IR JSON → deserialize struct → parse_tree → persist_tree → indexer) before touching code; the gap was unambiguous at parse_tree and the fix is exactly one node insertion.

## Evaluation notes (flywheel)

- Failure modes observed: silent data loss between deserializer and tree builder; no test caught it because the persist test only counted articles by type and was tuned to the importer's 312 — adding a positive content assertion (Preamble text present) prevents this class of regression.
- Graders run and results (PASS/FAIL): pending — A4 re-run on Fact 5 after user re-indexes will be the real grade.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after re-index, hit Fact 5 across all three registers (formal/casual/imperative) and confirm fast-path HIT in the trace + Preamble text in the answer.
