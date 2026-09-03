---
description: "Task list for feature 025 — Agentic Deep Search"
---

# Tasks: Agentic Deep Search

**Input**: Design documents from `specs/025-agentic-deep-search/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ (all present); ADR-0021 (Accepted)

**Tests**: INCLUDED — the architect requested unit tests per the contract acceptance checks (facet-expansion,
coverage-check, packing, grounding verifier, loop invariants).

**Organization**: grouped by user story (US1–US4 from spec.md). MVP = US1 behind the `ECHO_AGENTIC_DEEP` flag.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no incomplete-task dependency)
- **[Story]**: US1–US4 (foundational/setup/polish carry no story label)
- All paths are repo-relative under `src-tauri/` unless noted. Build/test on the VM
  (`cargo test --lib --no-default-features --features llm,server,cuda deep_search`).

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: module skeleton + flag + eval scaffold. No behavior change yet.

- [X] T001 Create the feature module skeleton `src-tauri/src/services/deep_search/{mod.rs,expand.rs,coverage.rs,assemble.rs,verify.rs}` with stubbed `pub(crate)` fns and doc-comments citing spec FRs; register `pub mod deep_search;` in `src-tauri/src/services/mod.rs`.
- [X] T002 Add the flag reader `deep_search::agentic_enabled()` in `src-tauri/src/services/deep_search/mod.rs` reading `ECHO_AGENTIC_DEEP` (`1|on|true|yes`), mirroring the `graph_expand_enabled()` env pattern; default OFF. (FR-008, R6)
- [X] T003 [P] Scaffold the A/B eval harness under `research/025-agentic-deep-search/` (README + `gold.jsonl` placeholder of 5–10 multi-part legal questions + a runner script that hits legacy vs agentic deep and records coverage/citations/latency). (SC-001..004, SC-006)

**Checkpoint**: module compiles with stubs; flag readable; eval folder exists.

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ CRITICAL**: no user-story work starts until this phase is complete.

- [X] T004 **(revised — COMPOSE, not extract; architect decision 2026-07-13)** Add `pub(crate) async fn retrieve_for_query(pool, embedder, notebook_id, scope, query_text, top_k) -> Vec<(id,source,content,page,label)>` in `src-tauri/src/services/query_router.rs`, **composing** the existing private helpers (`get_context_chunks` + `fts5_retrieve` + `union_merge`/`rrf_merge`, then the `summary_recall` and `graph_expand` arms) — SAME logic as the main path. `route_scoped_with_override` (a ~1,400-line hot fn) is **left untouched**, so normal search is byte-for-byte unchanged. Tradeoff: arm-composition now exists in two places (keep in sync). (R1, FR-002; deviates from ADR-0021 "extract" wording — see ADR addendum.)
- [X] T005 **(satisfied by construction)** Because T004 composes rather than extracts, `route_scoped_with_override` is unmodified → the normal path cannot regress (no snapshot diff needed). A live retrieval smoke-test for `retrieve_for_query` needs a test PG and folds into the US1 loop integration test. (FR-008, INV-7, SC-005)
- [ ] T006 [P] Expose reused helpers to the feature: make `summary_builder::pack_windows` `pub(crate)`; confirm `intent_router::decompose::detect_and_split`, the `DEEP_SEARCH` task-local, and the cancellation token are reachable from `deep_search`. (R4, R8)
- [X] T007 [P] Define the in-memory types per `data-model.md` in `src-tauri/src/services/deep_search/mod.rs`: `SubQuestion`, `Coverage` enum, `CoverageItem`/`CoverageReport`, `Passage`, `PassageSet`, `DeepTrace` (no DB, no serde-persistence). (data-model.md)
- [X] T008 **De-risk spike (front-loaded)**: ran the coverage-check prompt against vLLM on 3 known-ground-truth cases. **Result: GO** — latency avg 2.9 s (≤5 s budget), 3/3 JSON parsed, accuracy 9/11=81% with **both misses in the SAFE false-pending direction** (bias-to-refetch confirmed; zero false-answered → no omission risk). Findings + design implications in `research/025-agentic-deep-search/coverage-spike.md`. (R2, the one novel component)

**Checkpoint**: shared retrieval extracted + normal path green; types + reused helpers ready; coverage-check viability proven.

---

## Phase 3: User Story 1 - Complete answers to multi-part questions (Priority: P1) 🎯 MVP

**Goal**: the bounded loop (expand → round1 → coverage → round2-pending → synthesize) behind the flag, producing
an answer that covers every part of a multi-part question.

**Independent Test**: with `ECHO_AGENTIC_DEEP=1` and Deep ON, a 4-part question yields a distinct grounded
treatment of all 4 parts and addresses more parts than legacy deep; Deep OFF is unchanged.

### Tests for User Story 1 ⚠️ (write first, ensure fail)

- [X] T009 [P] [US1] Facet-expansion tests (`parse_facets_extracts_and_dedups`, `parse_facets_empty_on_garbage`) in `expand.rs`. Fallback is degrade-to-original (detect_and_split ladder deferred — see T013 note). (contracts/facet-expansion.md; FR-001, FR-012) ✅ 7 lib tests green
- [X] T010 [P] [US1] Coverage-report tests (`missing_and_unknown_bias_to_pending`, `garbage_makes_all_pending_except_original`) in `coverage.rs`. (contracts/coverage-check.md; FR-003, FR-007) ✅
- [X] T011 [P] [US1] Assembly tests (`dedup_keeps_first_seen_and_packs_round2_first`, `char_budget_truncates_but_keeps_at_least_one`) in `assemble.rs`. (data-model PassageSet; FR-013) ✅
- [~] T012 [P] [US1] Loop-invariant: ≤3 LLM calls / ≤2 rounds are **guaranteed by construction** (no repeating call sites; round 2 gated on non-empty pending). A *runtime* assertion + test needs a mock DB/model → folded into **T023** (US3). Original-leg-in-round-1 is structural. (FR-005, INV-1..3)

### Implementation for User Story 1

- [X] T013 [US1] `expand.rs`: LLM facet-expansion via `generate_direct` (256 tok) + prepend original leg + cap 6 + dedupe. **NOTE:** fallback is LLM→`[original]` (single-facet degrade); the intermediate `detect_and_split` rung was dropped for MVP (avoids drafting-type coupling) — optional refinement. (FR-001, FR-009, FR-012)
- [X] T014 [US1] `coverage.rs`: coverage-check over round-1 **passages** (T008-spike refinement — no separate draft call, keeps ≤3 LLM calls), parse → `CoverageReport`, `pending_ids` with bias-to-refetch, original leg excluded. (FR-003, FR-007, FR-009)
- [X] T015 [US1] `assemble.rs`: `PassageSet::add_round/dedup/packed` — dedup by chunk_id, round-2/pending-first, char-budgeted (own packer, not `pack_windows` — passages are tuples not strings). (FR-013)
- [X] T016 [US1] `mod.rs::run_agentic_deep(...)`: expand → round-1 `retrieve_for_query` per sub-question → coverage → round-2 pending-only (skip if none) → synthesis → `(QueryResponse, bool)`; ≤3/≤2 by construction; cancel checks at each boundary. `DeepTrace` population = US4. (FR-002, FR-004, FR-005, FR-010, FR-014)
- [X] T017 [US1] Routed in **`guidance.rs`** (the actual chat retrieval call site, not `api/query.rs`): `deep_search_active() && agentic_enabled()` → `run_agentic_deep`; else the unchanged single-pass path. (FR-008)
- [X] T018 [US1] Fast-paths (greeting/intro, `@not-found`) fire in intent routing **before** guidance, so honored by construction; masking/carry-over unaffected; single-facet degrade handled in expand. (FR-012)

**Checkpoint**: MVP — flag-on Deep runs the full bounded loop and covers multi-part questions; flag-off/normal unchanged. Independently demoable.

---

## Phase 4: User Story 2 - Trustworthy, grounded answers (Priority: P1)

**Goal**: every citation/figure in the final answer traces to a retrieved passage; unverifiable content dropped
or the answer degrades to grounded-fallback.

**Independent Test**: audit a Deep answer's citations → 100% traceable; a fabrication-tempting query (sparse
rows) drops the unsupported claim instead of inventing it.

### Tests for User Story 2 ⚠️

- [X] T019 [P] [US2] Grounding-verifier tests in `verify.rs`: empty-passages/empty-answer → fallback; figure present in a passage → clean; foreign figure → flagged; separator variants (1,000,000≡1000000); single-digit list markers ignored. **SCOPE NOTE:** a fabricated *bare entity* ("Zoonology" class) can't be caught deterministically (needs NLI) — documented limitation; defended by passages-only synthesis + citations-from-passages instead. (SC-002, FR-006, INV-4) ✅
- [X] T020 [US2] Implemented `verify.rs` grounding gate: replicated the `one_ask::verify::normalize`-substring basis (not coupled to `ProvisionCard`); `check(answer, &[&Passage])` → `GroundingOutcome{answer, grounded, figures_flagged}`; digit-normalized foreign-figure detection. (R5, FR-006)
- [X] T021 [US2] Wired into `run_agentic_deep`: ungrounded → grounded-fallback with **no** citations (INV-4); grounded → answer + optional figure caveat + citations. `DeepTrace.grounding` population folds into US4 (T025). (FR-006, INV-4, Constitution XI)

**Checkpoint**: Deep answers are grounded by construction; US1+US2 both testable independently.

---

## Phase 5: User Story 3 - Predictable, bounded wait (Priority: P2)

**Goal**: bounded, predictable latency (~65 s median target); no unbounded iteration.

**Independent Test**: over a representative set, every Deep turn uses ≤3 calls/≤2 rounds and median latency
sits in the target band.

- [X] T022 [US3] Intermediate calls capped: expand + coverage use `max_tokens=256`; only synthesis uses `SYNTH_MAX_TOKENS=1280`. (FR-009, SC-003)
- [X] T023 [US3] `DeepTrace` counters tally each call/round; `debug_assert!(llm_calls<=3, retrieval_rounds<=2)` in `run_agentic_deep`; test `bound_counting_matches_the_loop_shape` verifies 3-call/2-round (pending) and 3-call/1-round (no-pending, round 2 skipped) shapes. (FR-005, SC-003, INV-1/2)
- [~] T024 [US3] Cancellation is checked at all 4 round boundaries (`cancelled()` after expand/round1/coverage+round2/synth) returning `(empty, true)` — verified by construction/read. A full async integration test needs a mock model+DB harness (not present); deferred to the live smoke test. (FR-014, INV-6)

**Checkpoint**: latency bounded and asserted.

---

## Phase 6: User Story 4 - Officer can see Deep is working (Priority: P3)

**Goal**: the trace shows sub-questions, per-part coverage, and round-2 triggers.

**Independent Test**: open the admin query trace on a Deep query → it lists sub-questions, coverage, round-2
triggers, and the call/round counts.

- [X] T025 [US4] `DeepTrace` populated in `run_agentic_deep` (sub-questions, per-sub-question coverage, round-2 triggers, `llm_calls`, `retrieval_rounds`) and emitted via `emit_trace` as a structured `F025 DeepTrace:` log line. (FR-011)
- [~] T026 [US4] Trace currently surfaces via the log line (greppable in the admin logs). Rich rendering into the `QueryTrace` step panel would need the loop to thread the owner's `&mut QueryTrace` (the loop doesn't take one yet) — deferred as a UI-polish follow-up; the observability data is captured. (FR-011)

**Checkpoint**: Deep is observable end-to-end.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T027 Run the A/B eval (`research/025-agentic-deep-search/`) on the gold set: coverage vs human gold (SC-001 ≥90%), citation traceability (SC-002 =100%), latency median/p95 (SC-003), enumerate-a-table gain (SC-004), rater completeness preference (SC-006). Record results.
- [ ] T028 [P] Append the `CHANGELOG_DEV.md` entry (symptom/why, files+fns, cross-cutting) and run `quickstart.md` validation.
- [ ] T029 Retire the legacy one-pass deep path and remove the `ECHO_AGENTIC_DEEP` flag branch **only after** T027 clears SC-001/002/003/006 (guarded cutover). (spec: replaces one-pass deep)
- [ ] T030 [P] Verify the frontend Deep toggle copy/labels still read correctly with the new behavior (no functional change expected; the button/message shipped in `0b4607aa`).

---

## Dependencies & Execution Order

### Phase dependencies
- **Setup (P1)** → no deps.
- **Foundational (P2)** → after Setup; **blocks all user stories**. T008 (coverage spike) gates US1.
- **US1 (P3, MVP)** → after Foundational.
- **US2 (P4)** → after US1 (wraps the synthesis output); verifier (T020) can be built in parallel with US1 impl, wired in T021.
- **US3 (P5)** → after US1 (asserts on the loop).
- **US4 (P6)** → after US1 (reads loop state).
- **Polish (P7)** → after the desired stories; T029 gated on T027.

### Within a story
- Tests (T009–T012, T019) written first and failing before their implementation tasks.
- `expand`/`coverage`/`assemble` (T013–T015) before the orchestrator `mod.rs` (T016) that composes them.
- Orchestrator (T016) before routing (T017) and before US2/US3/US4 wiring.

### Parallel opportunities
- Setup: T003 ∥ (T001→T002).
- Foundational: T006 ∥ T007 (different files) after T004; T005 after T004; T008 after T007.
- US1 tests T009/T010/T011/T012 all ∥ (different files).
- US2 verifier T020 ∥ US1 impl (different file), joined at T021.

---

## Parallel Example: User Story 1 tests

```bash
# Launch the US1 unit-test tasks together (different files):
Task: "T009 facet-expansion tests in services/deep_search/expand.rs"
Task: "T010 coverage-report tests in services/deep_search/coverage.rs"
Task: "T011 assembly dedup/packing tests in services/deep_search/assemble.rs"
Task: "T012 loop-invariant tests in services/deep_search/mod.rs"
```

---

## Implementation Strategy

### MVP first (US1 only)
1. Phase 1 Setup → 2. Phase 2 Foundational (incl. T004 extraction + T005 guard + T008 spike) →
3. Phase 3 US1 → **STOP & VALIDATE** flag-on Deep covers multi-part questions, flag-off unchanged → demo.

### Incremental delivery
US1 (MVP, flag off in prod) → US2 grounding hardening → US3 latency bounds → US4 observability → Polish + A/B →
guarded retirement of legacy deep (T029).

---

## Requirement / Success-Criteria coverage

| FR | Task(s) | | SC | Task(s) |
|----|---------|---|----|---------|
| FR-001 | T009, T013 | | SC-001 | T027 (via US1 loop) |
| FR-002 | T004, T016 | | SC-002 | T019, T020, T021 |
| FR-003 | T010, T014 | | SC-003 | T022, T023, T024 |
| FR-004 | T016 | | SC-004 | T027 (enumerate-a-table) |
| FR-005 | T012, T016, T023 | | SC-005 | T005 |
| FR-006 | T019, T020, T021 | | SC-006 | T027 |
| FR-007 | T010, T014 | | | |
| FR-008 | T002, T005, T017 | | | |
| FR-009 | T013, T014, T022 | | | |
| FR-010 | T016 | | | |
| FR-011 | T025, T026 | | | |
| FR-012 | T009, T013, T018 | | | |
| FR-013 | T011, T015 | | | |
| FR-014 | T016, T024 | | | |

## Notes
- No DB migration, no window/GPU change (Out of Scope). Loop is stateless per turn.
- Build/test on the VM; `cargo build --lib` may hit the known `.so` stderr-relocation false error → use
  `cargo test --lib … deep_search`.
- Deploy = push branch:release; keep the flag OFF in prod until the A/B (T027) passes.
