# Tasks: Unified Bureaucrat Chat Assistant

**Input**: Design documents from `/specs/015-bureaucrat-chat/` (spec.md, plan.md, research.md, data-model.md, contracts/assistant-commands.md)
**Branch**: `015-bureaucrat-chat` (off `master`)
**Tests**: Acceptance/invariant tests ARE requested (citation-grade contract) — included as explicit tasks per Success Criterion.

> **REBASE NOTE**: This branch is off `master` and does NOT contain the F013 ingestion fixes (committed on branch `013-ingestion-hardening`). Rebase 015 on `master` **after** F013 merges, before starting implementation — capabilities US6/US8 depend on F013 amendment provenance (`amended_by` edges / `footnote_refs`).

## Format: `[ID] [P?] [Story] Description with file path`
- **[P]**: parallelizable (different files, no incomplete-dep)
- **[Story]**: US1–US9 for story-phase tasks; Setup/Foundational/Polish carry none.

---

## Phase 1: Setup

- [X] T001 Create `src-tauri/src/services/assistant/mod.rs` module skeleton (turn orchestration entry) and register it in `src-tauri/src/services/mod.rs`
- [X] T002 [P] Add additive migration `src-tauri/src/db/pg/migrations/004_pinned_case_fact.sql`: `pinned_case_fact` table + partial index **and** additive `conversation_id` column on `chat_message` (backfill from `draft_session_id`) per data-model.md (resolves analyze-I1) — ids TEXT to match `draft_session_id`
- [X] T003 [P] Create persona config scaffold `src-tauri/config/personas/bureaucrat.toml` (persona + per-capability prompt fragments + compact-persona variant) — Constitution VI
- [X] T004 [P] Create `src-tauri/src/commands/assistant.rs` and register commands in `src-tauri/src/lib.rs` (empty handlers for now)

---

## Phase 2: Foundational (BLOCKING — shared spine for all stories)

- [X] T005 Implement dynamic `n_ctx` resolver in `src-tauri/src/services/assistant/n_ctx.rs`: reads `model.n_ctx_train()`; GQA-aware KV-cache clamp vs `hardware_detector` RAM/VRAM; `ECHO_N_CTX` override; fallback 4096 (research R1). 6 pure unit tests. Wiring into llm.rs = T011.
- [X] T006 [P] Real-token counter `ModelTokenCounter` in `src-tauri/src/services/assistant/n_ctx.rs` wrapping `model.str_to_token`; `TokenCounter` trait in `context_budget.rs` (research R2). Intra-turn caching deferred to T014 orchestration.
- [X] T007 Implement the priority context allocator in `src-tauri/src/services/assistant/context_budget.rs`: partition `n_ctx` as gen-reserve → persona → pinned facts → question → retrieved law → recent turns → summary; assert `Σ ≤ n_ctx` before build; emit the budget plan to the query trace (FR-010..FR-013) — DONE: allocator + `TokenCounter` trait + 6 invariant unit tests (atomic law, law-outranks-history, overflow signal). Trace emission deferred to T047. (Partially covers T006 trait + T025 allocator tests.)
- [X] T008 Enforce **atomic provisions** in retrieval assembly: in `src-tauri/src/services/query_router.rs` remove the per-chunk 25%-of-budget cap; a provision is included whole or omitted (FR-014); law slot fills before history (FR-015)
- [X] T009 Extract the `[CITE:n]` verbatim-substring gate + refuse-or-flag from `src-tauri/src/services/drafting/one_ask/verify.rs` into `src-tauri/src/services/assistant/citation_gate.rs` (keep behavior; add `offer_to_index` flag output) (research R10) — re-homed the `normalize` + provision-scan + verbatim-substring primitives into a generic `gate(answer, context)` over free prose; `verify.rs` left intact (retired in T030). 5 unit tests.
- [X] T010 Extend intent classifier/dispatch: deterministic capability classification (guidance|draft|summarize_doc|compare|conflict|amendment_chain|action_items) + document-scope parse; default guidance (research R3) — implemented in `src-tauri/src/services/assistant/intent.rs` (cohesive with the F015 module) REUSING the proven `intent_router::classifier` + 2 new cues (draft, conflict). 6 unit tests.
- [X] T011 Wire dynamic `n_ctx` + pre-build `prompt ≤ n_ctx` assertion into `src-tauri/src/services/llm.rs` generation path (replace hardcoded 1024/2048/3584; the head+tail truncation becomes an unreachable safety net)
- [X] T012 Load persona/per-capability prompts from `bureaucrat.toml` in `src-tauri/src/services/assistant/mod.rs` (Constitution VI); compact variant selected on small windows
- [X] T013 Conversation memory base in `src-tauri/src/services/assistant/memory.rs`: load recent `chat_message` turns + `conversation_summary` for the allocator (pinned facts + eviction stubbed; filled in Phase E)
- [X] T014 Turn orchestration in `src-tauri/src/services/assistant/mod.rs`: classify → retrieve → allocate → generate → gate → assemble response; implement `send_message` in `src-tauri/src/commands/assistant.rs` with `assistant://progress`/`assistant://token` streaming (contracts)
- [X] T015 [P] Frontend single chat surface `src/pages/ChatPage.tsx` + `src/api/assistant.ts`: send/stream, render answer + citations + offer + flags + `context_fill` (replaces separate chat + drafting panes)

**Checkpoint**: foundation compiles; a guidance turn round-trips with citations and a logged budget plan (`Σ ≤ n_ctx`).

---

## Phase 3: User Stories 1–3 — Grounded, no-truncation MVP (P1) [Phase A]

**Goal**: general-answer-first cited guidance, zero confabulation, provable no-truncation.
**Independent test**: ask an in-corpus procedural question → complete cited answer + offer-to-tailor; ask an out-of-corpus question → flag + offer-to-index; long conversation → no overflow, whole laws.

- [X] T016 [US1] Implement guidance capability (single-pass) in `src-tauri/src/services/assistant/capabilities/guidance.rs`: retrieve → allocate → grounded answer
- [X] T017 [US1] General-answer-first + explicit offer-to-tailor behavior (prompt fragments in `bureaucrat.toml`, assembly in `guidance.rs`) (FR-003/FR-004)
- [X] T018 [US2] Wire citation gate into guidance: ungrounded claim → suppress + `flags[] = {missing, offer_to_index}` (FR-006/FR-007) in `capabilities/guidance.rs`
- [X] T019 [P] [US2] Approval-pipeline guidance grounded-or-flagged (FR-008) — prompt + gate coverage in `guidance.rs`
- [X] T020 [P] [US1] Frontend: offer-to-tailor affordance + flag/offer-to-index UI in `src/pages/ChatPage.tsx`
- [X] T021 [US3] Acceptance test SC-003 (zero window overflow): `src-tauri/tests/bureaucrat_chat_acceptance.rs` — stress conversation; assert every turn's assembled prompt ≤ n_ctx and no truncation warning
- [X] T022 [P] [US3] Acceptance test SC-002 (no truncated law): assert every retrieved provision in context is whole (no sliced provision) in `bureaucrat_chat_acceptance.rs`
- [X] T023 [P] [US2] Acceptance test SC-001 (zero fabricated citations): in/out-of-corpus set; assert every emitted number/quote is in `citations[].verbatim`; out-of-corpus → flag, 0 fabrications
- [X] T024 [P] [US1] Acceptance test SC-005 (first-answer completeness): in-corpus questions return a complete cited answer on turn one with no clarifying-question gate
- [X] T025 [US1] Unit tests for allocator + n_ctx resolver in `src-tauri/src/services/assistant/context_budget.rs` / `n_ctx.rs` (priority order, Σ≤n_ctx, clamp, ECHO_N_CTX) — 12 tests green

**Checkpoint (MVP)**: US1+US2+US3 green — a grounded, no-truncation assistant.

---

## Phase 4: User Stories 4–5 — Drafting + hot-swap model (P2) [Phase B]

- [X] T026 [US4] Draft capability in `src-tauri/src/services/assistant/capabilities/draft.rs`: gather-situation → plain-prose cited draft + approval pipeline; **larger generation reserve** (FR-013) — dedicated dispatch seam delegating to the shared single-pass engine with `Capability::Draft` (draft persona fragment + `gen_reserve_draft`, no offer-to-tailor); 3 unit tests
- [X] T027 [P] [US4] `export_draft` command in `src-tauri/src/commands/assistant.rs` reusing `src-tauri/src/services/drafting/export.rs` (plain text, no letterhead) (FR-022) — wraps the persisted draft prose in a single body `Block::Prose` → `draft_to_text`; `message_id` surfaced on `TurnResponse`
- [X] T028 [US5] Strip reasoning spans (`<think>…</think>`) from user-facing answer AND citation-gate input at the generation boundary in `src-tauri/src/services/llm.rs` (FR-009, research R7) — `strip_reasoning_spans` applied to `infer_with_cancel_temp` output at all return points; 3 unit tests
- [X] T029 [US5] `model_status` command + dynamic-window surfacing in `src-tauri/src/commands/assistant.rs` (model_file, n_ctx, trained_ctx, source) (FR-010/FR-019) — `ModelSlot::window_info` (real + stub); frontend header shows `model · n_ctx ctx`
- [X] T030 [US4] Retire slot/template/docset/gap/finalize from the user-facing path: remove `commands/drafting.rs` + `commands/one_ask.rs` registrations and the drafting/one-ask frontend pages/components (keep extracted gate + `export.rs`; leave tables/services dormant per additive policy) (FR-023) — dropped registrations + `DraftSessions` state + `commands::{drafting,one_ask}` modules; deleted frontend drafting/one-ask cluster; `ChatPage` is now the single `/` surface
- [X] T031 [P] [US5] Acceptance test SC-007 (window scales with model): force two `ECHO_N_CTX` values / two model files; assert retained law/history scales and grounding unchanged
- [X] T051 [P] [US5] Acceptance test FR-020 (model-independent grounding parity): run the same in/out-of-corpus set across two model files; assert citation-gate behavior (verbatim citations, flag-on-ungrounded) is identical regardless of model size/type, in `src-tauri/tests/bureaucrat_chat_acceptance.rs`

**Checkpoint**: drafting produces cited plain prose; model swap changes the usable window; thinking models render clean.

---

## Phase 5: User Stories 6 + 8 — Whole-doc summarize + amendment chain (P2) [Phase C]

- [X] T032 Map-reduce driver in `src-tauri/src/services/assistant/mapreduce.rs`: window-sized atomic batches (no provision split) → sequential per-batch extract on the single slot → reduce/synthesis; progress-streamed (Constitution III, research R4) — pure `plan_batches` (full-coverage invariant, 5 unit tests) + async `run` over `infer_with_cancel_temp`, reusing `query_router::build_rag_prompt`
- [X] T033 [US6] Summarize-doc capability in `src-tauri/src/services/assistant/capabilities/summarize_doc.rs` over the map-reduce driver; cited, covers all major parts (FR-005d) — gathers **leaf** provisions per source (no parent double-count) → map-reduce → gate+assemble; wired in `send_message`
- [X] T034 [US8] Amendment-chain capability in `src-tauri/src/services/assistant/capabilities/amendment_chain.rs`: build deterministically from the `amendment_events` table (NOT `amended_by` edges — that's where F013 provenance actually lives), ordered oldest→recent; pure `build_chain`/`render_prose` (4 unit tests), 0 model-invented links (FR-005f, research R6)
- [X] T035 [P] [US8] `get_amendment_chain` command in `src-tauri/src/commands/assistant.rs` (contracts) — deterministic chain via `graph::amendment_events_for_provision` + `build_chain`; registered in `lib.rs`
- [X] T036 [P] [US6] Acceptance test SC-009 (whole-document coverage): summary of a multi-section act covers 100% of major parts, each cited, none skipped — 60-section act, batcher union == 0..N exactly, each provision whole
- [X] T037 [P] [US8] Acceptance test SC-010 (amendment-chain fidelity): order oldest→recent + dates/instruments match provenance; 0 model-invented links — out-of-order provenance ordered correctly + "none recorded" path

**Checkpoint**: a whole act summarizes without dropping sections; amendment history is deterministic + cited.

---

## Phase 6: User Stories 7 + 9 — Compare/conflict + action items (P2/P3) [Phase D]

- [X] T038 [US7] Compare capability in `src-tauri/src/services/assistant/capabilities/compare.rs`: document-scoped retrieval → topic-aligned cited comparison units (FR-005e)
- [X] T039 [US7] Conflict capability in `src-tauri/src/services/assistant/capabilities/conflict.rs`: candidate contradictions citing BOTH provisions, framed for human judgment; never asserted (FR-005g, research R8)
- [X] T040 [P] [US9] Action-items capability in `src-tauri/src/services/assistant/capabilities/action_items.rs` (map-reduce): each item cites its mandating provision
- [X] T041 [P] [US7] `confirm_documents` command in `src-tauri/src/commands/assistant.rs` for scope disambiguation (FR-005e)
- [X] T042 [P] [US7] Acceptance test SC-011 (conflicts are cited candidates): 100% cite both provisions; 0 conflicts asserted without both citations

**Checkpoint**: cross-document compare/conflict + action items all grounded.

---

## Phase 7: Conversation memory polish (P2/P3) [Phase E]

- [X] T043 [US3] Pinned-fact extraction in `src-tauri/src/services/assistant/memory.rs`: labelled-pattern extraction → `pinned_case_fact` rows; supersede on restatement; always-present allocator slot (FR-018, research R5)
- [X] T044 [US3] Summarize-old-turns eviction in `src-tauri/src/services/assistant/memory.rs`: fold oldest turns into `conversation_summary` under window pressure; pinned facts exempt (FR-017)
- [X] T045 [US3] Context-fill notice (FR-018a) in turn response + `src/components/ContextFillNotice.tsx`; `new_chat_from_notice` command carrying non-superseded pinned facts + recap (FR-018b)
- [X] T046 [P] [US3] Acceptance test SC-004 (memory fidelity): facts stated early honored late across a long session; SC-008 (context-fill notice shown at threshold + facts carried)
- [X] T052 [US3] Acceptance test FR-021 (session survives restart): persist turns + `conversation_summary` + `pinned_case_fact`, simulate restart, assert the conversation + pinned facts reload intact, in `src-tauri/tests/bureaucrat_chat_acceptance.rs`

**Checkpoint**: long sessions never lose load-bearing facts; users are nudged to fresh chats with facts carried.

---

## Phase 8: Polish & Cross-Cutting

- [X] T047 [P] Emit the full budget plan + capability + citation audit into the query trace for every turn
- [X] T048 [P] Persona/prompt tuning pass in `bureaucrat.toml` (recent-turns count, gen-reserve sizes, fill threshold defaults from Assumptions)
- [X] T049 [P] Update `MODULAR_IMPROVEMENT_LOG.md` / changelog with the F015 assistant layer
- [X] T050 Full suite green: `cargo test --lib` + `cargo test --test bureaucrat_chat_acceptance`; confirm all P1 invariant gates pass

---

## Dependencies & order

- **Setup (T001–T004)** → **Foundational (T005–T015)** block everything.
- Within Foundational: T005→T006→T007 (n_ctx→tokens→allocator); T009 (gate) and T010 (intent) are [P]; T011 depends on T005; T014 depends on T007/T009/T010/T012/T013.
- **Phase 3 (MVP)** depends only on Foundational. **Phases 4–7** each depend on Foundational + Phase 3; they are largely independent of each other (parallelizable across stories).
- **Phase 5 (US6/US8)** additionally depends on the F013 rebase (amendment provenance) and the map-reduce driver T032.
- **Polish (Phase 8)** last.

## Parallel execution examples
- Foundational: T002, T003, T004, T006 in parallel; T009 ∥ T010.
- Phase 3 acceptance gates T022/T023/T024 run in parallel after T016–T019.
- Phases 4, 5, 6, 7 can be staffed in parallel once Phase 3 is green (distinct capability files).

## Implementation strategy (MVP first)
1. **Ship Phase 3 (US1+US2+US3) as the MVP** — a grounded, no-truncation bureaucrat assistant. Acceptance-gate the hardest invariants (SC-001/002/003/005) before adding capabilities.
2. Add capabilities incrementally (B→C→D), each behind its own acceptance gate.
3. Memory polish (E) last — it improves long sessions but isn't required for the MVP.

**Total: 52 tasks** — Setup 4 · Foundational 11 · US1–US3 (MVP) 10 · US4–US5 7 (incl. T051 FR-020 parity) · US6+US8 6 · US7+US9 5 · Memory 5 (incl. T052 FR-021 restart) · Polish 4.
