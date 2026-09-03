# Implementation Plan: Agentic Deep Search

**Branch**: `025-agentic-deep-search` | **Date**: 2026-07-13 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/025-agentic-deep-search/spec.md`
**ADR**: [ADR-0021 Bounded Agentic Multi-Pass Deep Retrieval](../../history/adr/0021-bounded-agentic-multi-pass-deep-retrieval.md)

## Summary

Replace the one-pass "Deep" toggle with a **bounded agentic loop** — at most **3 LLM calls** and **2 retrieval
rounds** per turn — so Deep answers a broad question across *several fresh context windows* instead of one
saturated one. Flow: (1) decompose + LLM facet-expand the question into 4–6 sub-questions (always keep the
original as a retrieval leg); (2) retrieval round 1 over each sub-question through the **existing
`query_router` hybrid path** (vector + FTS + RRF + graph + summary arms); (3) a **draft + coverage-check** call
that judges each sub-question answered/pending (bias to re-fetch when unsure); (4) retrieval round 2 for the
*pending* sub-questions only; (5) a **final grounded synthesis** over the deduped, budget-packed union of both
rounds. Grounding is enforced by a small general verifier reusing the `one_ask` quote/number-check primitives.
No new storage, no window/GPU change, normal search untouched. Ships behind a flag for A/B against today's
one-pass Deep before the old path is retired.

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`); TypeScript + SolidJS frontend (Deep toggle
already exists — no new UI beyond wording already shipped in `0b4607aa`).
**Primary Dependencies**: existing only — served LLM (vLLM Qwen3-8B) via `query_router::generate_direct`;
`fastembed` (ONNX) embeddings; `sqlx` (Postgres in prod / sqlite-vec + FTS5 per constitution); the graph
overlay + `summary_recall` + `graph_expand` arms already wired in `query_router`; `regex_lite`. **No new crate.**
**Storage**: none new — the loop is **stateless per turn**; it only *reads* existing `chunks`, graph overlay,
and summary index. No migration.
**Testing**: `cargo test --lib` (unit: facet-expansion JSON parse + fallback, coverage-report parse +
bias-to-refetch, cross-round dedup/packing precedence, general grounding verifier); an offline A/B eval under
`research/025-agentic-deep-search/` (completeness + citation-traceability + latency) reusing the summarize-eval
harness pattern.
**Target Platform**: Linux headless server (`noviq-server`, the deep request path) + Tauri desktop; Deep runs on
the server query path.
**Project Type**: single Rust backend + SolidJS frontend (web/desktop). Backend-only change for this feature.
**Performance Goals**: Deep ~**65 s median / ≤ ~90 s p95** (bounded, opt-in). Normal (Deep OFF) latency
**unchanged**. Intermediate calls #1/#2 kept short (small `max_tokens`, structured output) so only call #3 is a
full generation.
**Constraints**: strictly bounded — **≤ 3 LLM calls, ≤ 2 retrieval rounds** per turn, no self-extension;
per-request concurrency-safe (task-local, no globals); grounded (no fabricated citations); cancellation at
round boundaries; smallest-viable-diff.
**Scale/Scope**: small corpus (~17 sources, ~7–8k chunks) + graph overlay (~616 edges). Single officer per
request; concurrent officers isolated by the per-request task-local.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Status | Notes |
|---|-----------|--------|-------|
| I | Privacy-First / Offline | ✅ PASS | All inference local (prod vLLM on-box); zero cloud, zero telemetry. No new outbound access. |
| II | Tiered Intelligence — cheapest first | ✅ PASS | Loop is **opt-in** (Deep only); normal path unchanged. Calls #1/#2 are cheap/short structured calls; only #3 is a full generation. No tier skipped for normal queries. |
| III | Single Model Slot | ✅ PASS | Calls are **sequential** on the one served model; never two LLMs resident. |
| IV | SQLite single source of truth | ✅ PASS (no change) | Feature adds **no** storage/schema. (Prod's Postgres port is a pre-existing, separately-tracked deviation — not introduced here.) |
| V | Pending Actions Queue | ✅ N/A | Read-only retrieval + generation; no dangerous tool calls. |
| VI | Persona-as-Config (TOML) | ✅ PASS | New prompts (facet-expansion, coverage-check) are prompt strings alongside existing grounding prompts; no new agent persona/tool. |
| VII | Hardware-Adaptive | ✅ PASS | Loop runs on the same served tier; degrades gracefully (single-facet → near single-pass). |
| VIII | Incremental Delivery | ✅ PASS | Flag-gated; P1 (multi-part completeness) alone is a shippable MVP; A/B before retiring old path. |
| IX | Pakistan Gov Legal Domain | ✅ PASS | No scope broadening; tuned to legal multi-part questions. |
| X | Officer-First UX | ✅ PASS | Directly optimizes the officer's completeness need on broad questions. |
| XI | Citation Discipline | ✅ **STRENGTHENED** | Grounding gate enforces ≥1 traceable citation and drops unverifiable claims (US2). |
| XII | Latest Text + Amendment Chain | ✅ PASS | Reuses the amendment-aware arms already in the retrieval path. |
| XIII | Cross-Reference Graph first-class | ✅ PASS | Each sub-question's retrieval uses the graph arm. |

**Development-constraint deviation (justified below in Complexity Tracking):** the ≤10 s Tier-3/4 p95 latency
budget (Development Constraints) does **not** hold for Deep — target is ~65 s median. Justified: Deep is an
explicit opt-in "thorough" mode; the budget governs default interactive queries, which are unchanged.

**Gate result: PASS** (one justified deviation; no unjustified violations).

## Project Structure

### Documentation (this feature)

```text
specs/025-agentic-deep-search/
├── plan.md              # This file
├── spec.md              # Feature spec
├── research.md          # Phase 0 — decisions (reuse map, coverage-check design, flag)
├── data-model.md        # Phase 1 — in-memory types (SubQuestion, CoverageReport, PassageSet)
├── quickstart.md        # Phase 1 — how to run/flag/eval the loop
├── contracts/           # Phase 1 — internal contracts (facet-expansion + coverage-check JSON, loop invariants)
│   ├── facet-expansion.md
│   ├── coverage-check.md
│   └── loop-orchestration.md
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/services/
├── deep_search/                 # NEW module — the bounded orchestrator (feature-local)
│   ├── mod.rs                   #   run_agentic_deep(...) — the ≤3-call/≤2-round loop
│   ├── expand.rs                #   LLM facet-expansion (extends decompose); JSON parse + fallback
│   ├── coverage.rs              #   coverage-check call + parse; bias-to-refetch; NEW component
│   ├── assemble.rs              #   cross-round dedup + budget packing (pending-first) via pack_windows
│   └── verify.rs                #   general grounding gate reusing one_ask quote/number primitives
├── query_router.rs              # EXTRACT reusable `retrieve_for_query(sub_q) -> selected chunks` from
│                                #   route_scoped_with_override; DEEP routes here instead of one-pass; the
│                                #   DEEP_SEARCH task-local + graph/summary arms stay as-is for the sub-calls
├── intent_router/decompose.rs   # REUSE detect_and_split; expand.rs adds the LLM-expansion layer on top
├── drafting/one_ask/verify.rs   # REUSE quote_verifies + foreign_number_in_prose (extract to shared if needed)
└── summary_builder.rs           # REUSE pack_windows (make pub(crate) if not already)

src-tauri/src/api/query.rs        # route deep=true → deep_search::run_agentic_deep (behind ECHO_AGENTIC_DEEP)
```

**Structure Decision**: A new **feature-local `services/deep_search/` module** owns the orchestration, keeping
the diff off the hot `query_router` path. The single structural change to shared code is **extracting a
reusable single-query retrieval function** from `route_scoped_with_override` so both the normal path and each
sub-question call share one retrieval engine (graph + summary + RRF + selection). Everything else is call-site
reuse of existing helpers.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Deep latency ~65 s median (exceeds the ≤10 s Tier-3/4 p95 dev-constraint budget) | The whole point is completeness on questions too broad for one window; multi-pass inherently costs more wall-clock. It is **opt-in** and clearly labeled "thorough, slower". | Staying within 10 s means staying single-pass — which the audit proved cannot see more law, i.e. cannot deliver the feature. Bounding to ≤3 calls/≤2 rounds caps the cost so it stays predictable rather than unbounded. |
| Extract a shared retrieval fn from `route_scoped_with_override` (touches a hot, complex function) | Each sub-question must run the *same* hybrid+graph+summary retrieval the normal path uses; duplicating it would drift. | Calling `one_ask::research()` was the seed idea but it is drafting-coupled (OneAskIntent/section skeleton/ProvisionCard/citation caps) — bending it to Q&A is more complex and riskier than a clean extract. |

## Phase 0 — Research

See `research.md`. Resolves: (R1) reuse boundary — `query_router` retrieval vs `one_ask::research` (decided:
extract from query_router; one_ask contributes only low-level verify/merge primitives); (R2) coverage-check
output contract + how to keep it cheap and hallucination-resistant; (R3) facet-expansion prompt + JSON schema +
fallback to rule-based `detect_and_split`; (R4) cross-round dedup + budget-packing precedence; (R5) grounding
gate design (general verifier from one_ask primitives); (R6) flag/rollback + A/B; (R7) latency budget
allocation across the 3 calls; (R8) cancellation + concurrency (task-local) model.

## Phase 1 — Design & Contracts

- `data-model.md` — in-memory types: `SubQuestion`, `CoverageItem`/`CoverageReport`, `PassageSet` (round-tagged,
  deduped), `DeepTrace`. No DB entities.
- `contracts/facet-expansion.md` — input (original query + slots) → output JSON (`sub_questions[]`, ≤6, plus the
  mandatory original-query leg); parse rules + fallback.
- `contracts/coverage-check.md` — input (draft + sub-questions + which passages fed each) → output JSON
  (`per_subquestion: {id, status: answered|pending, support_ref?}`); bias-to-refetch on parse/uncertainty.
- `contracts/loop-orchestration.md` — the bounded-loop invariants (≤3 LLM calls, ≤2 rounds, degrade rules,
  cancellation points, trace fields) as executable acceptance checks.
- `quickstart.md` — enabling `ECHO_AGENTIC_DEEP`, running a deep query, reading the trace, running the A/B eval.

## Phase 2 — (deferred to /sp.tasks)

Tasks will be generated by `/sp.tasks`. MVP = the P1 completeness path (expand → round1 → coverage → round2 →
grounded synthesis) behind the flag, with unit tests + a small A/B; P2/P3 add bounded-latency assertions and
trace surfacing.

## Progress Tracking

- [x] Phase 0 research complete (`research.md`)
- [x] Phase 1 design complete (`data-model.md`, `contracts/`, `quickstart.md`)
- [x] Constitution Check PASS (1 justified deviation)
- [ ] Phase 2 tasks (`/sp.tasks`)
