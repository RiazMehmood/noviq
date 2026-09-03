# Implementation Plan: Query Decomposition for Multi-Fact Questions

**Branch**: `008-query-decomposition` | **Date**: 2026-06-07 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/008-query-decomposition/spec.md`

## Summary

When a multi-fact legal question reaches a capacity-limited small model, decompose it
into single-fact sub-questions (rule-based), answer each through the existing RAG seam
(`query_router::route_scoped_with_override`) **sequentially** (one model resident at a
time), then assemble the verified sub-answers with a **deterministic template** — no LLM
merge. The path is gated on generator capability (active GGUF param size ≤ threshold,
hardware-profile fallback, operator override): off for 3B+/GPU deployments, which answer
multi-fact natively in one pass. Falls back to today's single-pass whenever a question is
not confidently multi-fact or yields fewer than two valid sub-questions.

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`)
**Primary Dependencies**: existing only — `regex_lite` (slot/marker parsing), `tokio`
(async seam + cancellation), `sqlx` (no schema change). No new crates.
**Storage**: N/A — no migration. Reuses `echo.db` read paths through the seam. Query
traces already persist; decomposition adds trace steps, not tables.
**Testing**: `cargo test -p echo-ai --lib` (unit, pure functions) + existing integration
harness; acceptance via the 15-prompt fundamental_rights eval (manual, in-app).
**Target Platform**: Tauri desktop (Linux/Windows), offline, CPU-first; small-model
deployments on 8–16 GB RAM.
**Project Type**: single (Rust backend within Tauri app); no frontend change.
**Performance Goals**: single-fact path unchanged (zero added latency); multi-fact path
~N× a single pass (N = sub-question count), accepted per spec.
**Constraints**: ≤1 LLM resident at any time (Principle III) ⇒ sub-questions answered
**sequentially**, never in parallel. Smallest viable diff; additive only.
**Scale/Scope**: first cut = two question shapes (`(a)(b)(c)` enumeration; 2+ named
provisions). Sub-question cap (sane upper bound, e.g. 6) to bound worst-case latency.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Privacy-First, Offline-Always** ✅ — all sub-question inference is local; no new
  network access.
- **II. Tiered Intelligence — Cheapest Tier First** ✅ — decomposition does NOT skip
  tiers; each sub-question runs the same tier ladder as a normal query. It issues *more*
  same-tier calls, justified because the single call is demonstrably wrong on small
  models (V3 eval). Gated so it never runs where unneeded.
- **III. Single Model Slot — Bounded RAM** ✅ (load-bearing) — sub-questions are answered
  **sequentially**; at most one model resident throughout. Parallel sub-question
  inference is explicitly forbidden by this plan.
- **IV. SQLite Single Source of Truth** ✅ — no new DB files; no migration.
- **V. Pending Actions Queue** ✅ — N/A (read-only query path, no DANGEROUS tools).
- **VI. Persona-as-Config** ✅ — N/A (no agent/tool change).
- **VII. Hardware-Adaptive Behavior** ✅ (direct embodiment) — the capability gate is a
  hardware-adaptive degrade: capable tiers bypass decomposition; `ECHO_*` override mirrors
  `ECHO_FORCE_TIER` precedent. Degrades gracefully, never silently fails.
- **VIII. Incremental Feature-App Delivery** ✅ — additive to App 1 (Notebook chat);
  US1/US2 each independently testable; US3/US4 are "no-change" guarantees.
- **IX. Pakistan Government Legal Domain** ✅ — operates on the existing legal corpus;
  no scope broadening.
- **X. Officer-First UX** ✅ — improves officer trust on multi-part questions; merged
  output is officer-grade labelled blocks.
- **XI. Citation Discipline** ✅ (must verify in design) — each merged block carries its
  sub-answer's citation(s); citation markers remapped to stay valid. No legal block ships
  uncited.
- **XII. Latest Text + Amendment Chain** ✅ — citations flow through unchanged from the
  seam; no truth-model change.
- **XIII. Cross-Reference Graph** ✅ — N/A (US1 fast/standard tier; no graph dependency).

**Development constraints**: smallest viable diff (new module + one orchestration hook in
`dispatch`); additive; every user-facing "not found" part carries a remediation message.

**Result**: PASS. One item flagged for design verification — citation remapping (XI).

## Project Structure

### Documentation (this feature)

```text
specs/008-query-decomposition/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output (types/contracts)
├── quickstart.md        # Phase 1 output (how to run + eval)
├── contracts/
│   └── decompose-api.md # Phase 1 output (internal module contract)
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/services/intent_router/
├── mod.rs          # MODIFIED: dispatch() gains the capability gate + decomposition
│                   #   orchestration around the existing route_scoped_with_override call.
├── slots.rs        # REUSED: section_paths drives multi-article detection (already there).
├── classifier.rs   # REUSED: unchanged.
└── decompose.rs    # NEW: capability gate, rule-based detect+split, deterministic merge,
                    #   citation remap. Pure functions + one async orchestrator helper.

src-tauri/src/services/query_router.rs
└── route_scoped_with_override(...)  # REUSED as-is (the per-sub-question seam).
   discover_local_gguf()  # REUSED: returns (path, label); label is the parsed param
                          # size ("0.5"/"1.5"/"3"). Gate parses label→f32. parse_param_size_b
                          # is private to query_router and is NOT called cross-module.

src-tauri/src/services/hardware_detector.rs
└── HardwareTier { ram_gb, has_discrete_gpu, max_model_tier }  # REUSED for gate fallback.
```

**Structure Decision**: Single Rust project. All new logic lives in one new file
`intent_router/decompose.rs` (pure, unit-testable functions: `decomposition_enabled`,
`detect_and_split`, `merge_subanswers`, `remap_citations`). The only mutation to existing
code is a wrapper in `intent_router::dispatch` that, when the gate is on and a split is
found, replaces the single `route_scoped_with_override` call with a sequential loop +
merge. This keeps the detector/splitter/merger independently testable and the diff small.

## Complexity Tracking

> No constitution violations. Decomposition issues N same-tier inference calls instead of
> 1; this is justified (single call is wrong on small models) and bounded (gated to small
> models, sub-question cap, sequential execution preserves the single-model-slot
> invariant). No simpler alternative achieves correct multi-fact answers on a 0.5B without
> a larger model.
</content>
