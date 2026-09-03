---
id: 0018
title: US10 — move-per-paragraph skeleton + advocacy strip (no truncation)
stage: green
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: "make it precise, one move per paragraph"; "without guard and go"
labels: [drafting, one-ask, secretariat-summary, skeleton, brevity, citation-grade]
links:
  spec: specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/drafting/one_ask/skeleton.rs
  - src-tauri/src/services/drafting/one_ask/brevity.rs
  - src-tauri/src/services/drafting/one_ask/state.rs
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - src-tauri/src/services/drafting/one_ask/compose.rs
  - src-tauri/src/services/drafting/one_ask/mod.rs
  - src-tauri/src/services/drafting/one_ask/doc_type.rs
  - src-tauri/tests/one_ask_acceptance.rs
  - specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
tests:
  - cargo test --lib → 441 passed, 0 failed
  - one_ask_acceptance 3/3 (rebuilt 7-section canned map + assertions)
  - one_ask_knn_parity 2/2
---

## Prompt

The live US9 draft is too long: each move should be its own concise paragraph; previous skeleton too
long. Keep one-line prayer + conditional financial. Initially "hard cap 2 sentences", then revised:
truncation could cut a sentence carrying the governing rule/citation — "don't truncate it or make it
small". Decision: no truncation guard — "without guard and go".

## Response snapshot

- Discussed the design; corrected the brevity approach after the architect flagged that a hard
  sentence cap risks cutting the governing rule (citation-grade violation). Settled: brevity via
  one-move-per-paragraph structure + a deterministic advocacy strip; NO truncation.
- Updated spec (US10 / FR-019..022; FR-020 = no truncation).
- Implemented the 7-section move-per-paragraph skeleton; removed US9's now-redundant `LegalMove`
  per-move machinery (each move is its own section → plain per-section caps). Reused US9 urgency +
  stakeholder extraction. Added `brevity::strip_advocacy` (verbatim-safe; prose-only; rules/figures
  preserved), wired into `compose::push_prose`. Conditional Financial (skip when no figures).
- Reworked the acceptance integration test (7-section canned-writer map, assertions, stage-flow
  totals). Fixed a mid-build bug: a tidy rule in strip_advocacy stripped a legitimate leading comma.

## Outcome

- ✅ Impact: the summary is now one tight move per paragraph (urgency → legal basis → stakeholders →
  status → procedure → financial-if-figures → one-line prayer), with advocacy padding stripped
  deterministically and every governing rule + citation preserved. Two-lane verbatim contract intact.
- 🧪 Tests: lib 441/0; acceptance 3/3; parity 2/2. New: brevity (3 tests) + reshaped skeleton/verify
  tests. US9's per-move machinery + its tests removed cleanly (no dangling refs across the crate).
- 📁 Files: 8 one_ask files + doc_type + acceptance test + spec.
- 🔁 Next prompts: live-validate (rebuild+relaunch, no re-index). Open index-time threads: §3 tabular
  garble + mislabel (US8 insufficient → re-diagnose + re-index); retrieval precision (Rule 14 vs 16,
  9.11 leak); gate stakeholder extraction by the relevance floor.
- 🧠 Reflection: the architect's pushback on truncation was correct and citation-grade-critical —
  brevity must never be allowed to delete a governing rule; the safe levers are structure + an
  advocacy strip that only ever touches prose, never a spliced citation.

## Evaluation notes (flywheel)

- Failure modes observed: advocacy strip's leading-conjunction tidy rule corrupted prose fragments
  beginning with ", " — removed. Integration-test fixtures are coupled to global CITE indices; the
  7-section card layout shifted them, so citation assertions were relaxed to a range (the unit tests
  carry the precise coverage).
- Graders run and results (PASS/FAIL): standalone strip harness PASS; cargo --lib PASS (441/0);
  acceptance + parity PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): if live drafts still ramble, add the citation-safe
  trailing-filler trim (drops only no-citation/no-figure trailing sentences) — deferred per architect.
