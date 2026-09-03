---
id: 035
title: sentence-level grounding validator task10
stage: green
date: 2026-05-16
surface: agent
model: claude-opus-4-7
feature: tier3-hierarchical-retrieval
branch: feature/tier3-hierarchical-retrieval
user: engr.riaz2010@gmail.com
command: recall memory and start working on task 10
labels: [grounding, jaccard, sentence-level, validator, task10]
links:
  spec: docs/superpowers/plans/2026-05-15-tier3-hierarchical-retrieval.md
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - path: src-tauri/src/services/grounding.rs
    change: created — validate() + GroundingReport/GroundingAction + 4 unit tests
  - path: src-tauri/src/services/mod.rs
    change: added pub mod grounding
  - path: CHANGELOG_DEV.md
    change: appended Task 10 dated entry with plan-drift notes
tests:
  - name: cargo test --lib services::grounding
    result: PASS — 4/4 tests pass (well_grounded, hallucinated→retry, one_bad→strip, no_citation→ship)
---

## Prompt

recall memory and start working on task 10

## Response snapshot

- Recalled Tier 3 memory: branch `feature/tier3-hierarchical-retrieval`, Tasks 1-9 complete, Task 10 is `services/grounding.rs` sentence-level Jaccard validator at plan lines ~1506+.
- Read plan lines 1506-1715 — got the full spec for `validate()`, `GroundingReport`, `GroundingAction { Ship, Strip, Retry }`, constants `JACCARD_THRESHOLD=0.25`, `MAX_DROPPABLE_UNGROUNDED=2`, the four required unit tests, and the policy thresholds.
- Created `src-tauri/src/services/grounding.rs` per plan with two deviations:
  1. `regex::Regex` → `regex_lite::Regex` (this repo only depends on regex-lite; pattern `\[(\d+)\]` needs no lookaround).
  2. `MIN_SHARED_TOKENS` 3 → 2: first run of `cargo test` failed `one_bad_sentence_strips` (got Retry, expected Strip). Analysis showed the supported sentence "Promotion requires five years of service in B-17 [1]" only shares {promotion, years} with the chunk after ≥4-char + stopword filtering — short tokens like "5", "B-17" are filtered out. With MIN=3 the supported sentence failed, pushing failed-count to 2/3 (>50% → Retry). Lowering MIN_SHARED_TOKENS to 2 makes the plan's own test pass and is also better behavior for short factual sentences.
- Registered `pub mod grounding;` in `src-tauri/src/services/mod.rs`.
- `cargo test --lib services::grounding` → 4/4 PASS.
- Appended Task 10 entry to `CHANGELOG_DEV.md` documenting both plan-drift items.

## Outcome

- ✅ Impact: Grounding validator API available for Task 11 to call from `query_router.rs`. Not yet wired into any query path.
- 🧪 Tests: 4 lib unit tests pass; no integration test needed.
- 📁 Files: 3 changed — grounding.rs (new), mod.rs, CHANGELOG_DEV.md.
- 🔁 Next: Task 11 — wire reranker + new `grounding::validate` into `query_router.rs` success path; raise `CONTEXT_TOKEN_BUDGET` to 2400; add reranker_slot to AppState; add `roll_up_siblings`.
- 🧠 Reflection: Plan code-as-spec is useful but small constant-tuning drift is expected when filtering rules interact with test data. Catching it in the first test run cheapens iteration vs. waiting until Task 11 wires it into production traffic.

## Evaluation notes (flywheel)

- Failure modes observed: 1 — `one_bad_sentence_strips` failed first pass due to MIN_SHARED=3 too strict for the plan's test fixture.
- Graders run and results: cargo test (lib, services::grounding) — PASS (after constant tuning).
- Prompt variant: N/A.
- Next experiment: Task 11 — measure end-to-end retrieval quality lift from reranker + sentence-grounding vs. RRF-only + paragraph-grounding baseline.
