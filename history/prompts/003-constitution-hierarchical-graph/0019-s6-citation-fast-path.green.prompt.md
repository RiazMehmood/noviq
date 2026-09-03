---
id: 0019
title: S6 T025 citation fast-path + IR relocation
stage: green
date: 2026-05-22
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: Riaz Mehmood
command: (conversation)
labels: [constitution, retrieval, citation-fast-path, query-router]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - src-tauri/src/services/query_router.rs
  - tools/constitution_import/constitute_IR/constitution_ir.json
  - src-tauri/tests/structure_parser_test.rs
  - src-tauri/src/services/structure_parser.rs
  - src-tauri/src/services/graph_builder.rs
  - specs/003-constitution-hierarchical-graph/quickstart.md
  - specs/003-constitution-hierarchical-graph/tasks.md
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib -- citation persist graph_builder — 15 passed / 0 failed
  - cargo test --test structure_parser_test — 3 passed / 0 failed
---

## Prompt

[The user said "continue into S6". During the build the IR-loading tests failed
"No such file or directory" — the user had moved `constitution_ir.json` into a
sub-folder. The user explained the move isolates the IR for folder-scoped
indexing, and asked whether fixing the folder-name typo would affect the
already-indexed data.]

## Response snapshot

Implemented T025: `detect_citation` (regex `Article N(x)(y)` → canonical
`unit_label`) and `citation_fast_path` (resolve by `unit_label`, return the
unit + its `unit_path`-prefixed sub-tree) in `query_router.rs`, with 2 tests.

A build hit `No such file or directory` — the user had moved the IR JSON into a
`constutute_IR/` sub-folder (a sensible move: folder-scoped indexing was
otherwise sweeping in the crate's own source files). Confirmed the rename does
NOT affect already-indexed `echo.db` data (keyed by source_id, not path), fixed
the typo (`constutute_IR` → `constitute_IR`), and repointed all 6 references
(4 test paths + quickstart + importer doc-comment).

All tests green: 15 lib + 3 integration. T025 NOT yet wired into `route_scoped`
— that is T026.

## Outcome

- ✅ Impact: T025 done — the deterministic citation lookup that fixes F1/F2/F4
  exists and is tested. IR path canonicalised to `constitute_IR/`.
- 🧪 Tests: `cargo test --lib -- citation persist graph_builder` 15/0;
  `structure_parser_test` 3/0.
- 📁 Files: query_router.rs (+detect_citation/citation_fast_path/tests); IR
  JSON moved + 6 path references repointed; tasks.md, CHANGELOG_DEV.md.
- 🔁 Next prompts: T026 — wire `citation_fast_path` + 1-hop `graph::expand`
  into `route_scoped` (hot path; decide graph-expansion gating); T027 lib.rs;
  T028 F1–F5 eval (A4).
- 🧠 Reflection: a test failure surfaced an out-of-band working-tree change
  (the user's file move) — investigating the *why* (folder-index pollution)
  rather than just repointing paths produced the right canonical fix.

## Evaluation notes (flywheel)

- Failure modes observed: `regex_lite` `captures` needs a bound (not temporary)
  input string — E0716; the moved IR file broke 4 test paths.
- Graders run and results (PASS/FAIL): PASS (15 lib + 3 integration).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T026 — fast-path short-circuit in
  `route_scoped` after the cache check.
