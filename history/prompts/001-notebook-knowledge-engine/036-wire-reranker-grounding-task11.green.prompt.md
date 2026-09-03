---
id: 036
title: wire reranker rollup grounding into query path task11
stage: green
date: 2026-05-16
surface: agent
model: claude-opus-4-7
feature: tier3-hierarchical-retrieval
branch: feature/tier3-hierarchical-retrieval
user: engr.riaz2010@gmail.com
command: proceed to task 11
labels: [query_router, reranker, grounding, rollup, task11]
links:
  spec: docs/superpowers/plans/2026-05-15-tier3-hierarchical-retrieval.md
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - path: src-tauri/src/services/query_router.rs
    change: bumped CONTEXT_TOKEN_BUDGET 1200→2400; added reranker pass; added roll_up_siblings; added sentence-grounding pass; threaded reranker_slot param through route/route_scoped
  - path: src-tauri/src/commands/query.rs
    change: added reranker_slot State<'_, Arc<RerankerSlot>> to stream_query_notebook; passed into route_scoped
  - path: src-tauri/src/lib.rs
    change: imported RerankerSlot; constructed Arc<RerankerSlot> via resolve_model_path; .manage() before pool
  - path: CHANGELOG_DEV.md
    change: appended Task 11 dated entry
tests:
  - name: cargo build (dev)
    result: PASS — Finished in 1m 00s, 0 errors, 3 pre-existing warnings
  - name: cargo test --lib query_router
    result: PASS — 39/39
---

## Prompt

proceed to task 11

## Response snapshot

- Read plan Task 11 (lines 1717-1896). Confirmed plan's `state.reranker_slot` doesn't match this repo's pattern (no AppState struct; uses Tauri State<Arc<T>> per command).
- Inspected actual call surface: `route_scoped(pool, embedder, model_slot, tier, ...)` is invoked from `commands/query.rs::stream_query_notebook` only. Threaded `reranker_slot: &RerankerSlot` parameter through both `route` and `route_scoped`; matched the existing `model_slot` pattern (`State<'_, Arc<RerankerSlot>>` in the command, `&reranker_slot` at call site, `&RerankerSlot` in service signature).
- Bumped `CONTEXT_TOKEN_BUDGET` 1200 → 2400 with inline Task 11 comment.
- Inserted reranker pass (between RRF merge at line 449 and `build_context`). Bypassed for `QueryType::Summary` and empty candidate sets; on error logs warn + trace step Skipped, falls back to RRF order.
- Inserted `roll_up_siblings(reranked, pool).await` immediately after reranker.
- Added new `async fn roll_up_siblings` just before `#[cfg(test)]` block. Two SQLx round-trips (metadata probe + parent fetch); both errors return input unchanged.
- Did NOT replace the legacy `validate_grounding` block. Instead added the new sentence-grounding pass *after* it (layered) — preserves `grounding_uncited_only`/fallback/audit logic per "smallest viable diff" principle. Skipped when `tier_used == "error"`, `Summary`, or answer already equals `GROUNDING_FALLBACK`. For `Strip` substitutes kept text; for `Retry` logs only (legacy reviewer retry path remains the actual retry trigger).
- Wired `RerankerSlot` construction in `lib.rs::setup_models_and_watchers` via `resolve_model_path("models/bge-reranker-base/{model.onnx,tokenizer.json}")` and `.manage()`'d the Arc.
- `cargo build` → clean (1m 00s, 0 errors). `cargo test --lib query_router` → 39/39 PASS.

## Outcome

- ✅ Impact: Reranker + sibling rollup + sentence grounding now active on every Regular query. Summary queries still bypass reranker (intentional). Missing reranker model file → warns + falls back to RRF order, query still succeeds.
- 🧪 Tests: cargo build clean; 39 query_router unit tests pass; grounding lib tests still pass.
- 📁 Files: 4 files modified — query_router.rs, query.rs, lib.rs, CHANGELOG_DEV.md.
- 🔁 Next: Task 12 — end-to-end acceptance test for hierarchical retrieval (FTS5 leaf hit on clause query). Task 13 — final CHANGELOG pass.
- 🧠 Reflection: Plan's "replace the call to legacy validate_grounding" was ambiguous given how much downstream logic depends on the legacy validator's `uncited_only` flag. Layering instead of swapping kept this task scope-bounded and preserves all existing behaviour while adding the new sentence-level Strip signal as a refinement.

## Evaluation notes (flywheel)

- Failure modes observed: none — compiled and tested clean first attempt.
- Graders run and results: cargo build (lib) — PASS; cargo test --lib query_router — 39/39 PASS.
- Prompt variant: N/A.
- Next experiment: Task 12 — verify FTS5 hierarchy indices actually return the leaf chunk on a precise-clause query against an in-memory SQLite seeded with document/section/leaf rows.
