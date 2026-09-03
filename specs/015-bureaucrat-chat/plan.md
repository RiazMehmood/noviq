# Implementation Plan: Unified Bureaucrat Chat Assistant

**Branch**: `015-bureaucrat-chat` | **Date**: 2026-06-21 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/015-bureaucrat-chat/spec.md` · ADR-0016

## Summary

Replace the separate chat pane and the drafting slot/template subsystem with **one intent-routed conversational surface** under a permanent bureaucrat persona. The same surface serves guidance, drafting, whole-document summarize, cross-document compare/conflict, amendment-chain, and action-items — every capability bound by the same four invariants: bureaucrat persona, verbatim `[CITE:n]` citation gate, a real-token **priority context allocator** that guarantees `prompt ≤ window` before generation (so truncation never fires), and **deterministic-where-truth-is-structural**. The model is hot-swappable; `n_ctx` is derived from the model + hardware. Technical spine: extend `intent_router`/`query_router`; add a context-allocator module + dynamic-`n_ctx` resolver replacing today's hardcoded per-call-site `n_ctx`; re-home the one-ask citation gate; add pinned-facts + summarize-old-turns memory; drive the amendment chain from F013 provenance.

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`) + TypeScript/SolidJS frontend
**Primary Dependencies**: Tauri 2.0, llama-cpp-2 0.1.143 (free-text generation; **no llguidance** — proven to crash), fastembed 4, sqlx 0.8 (PostgreSQL feature, runtime queries), pgvector, regex_lite, serde/serde_json, tokio. Reused: `query_router`, `intent_router` (decompose + dispatch), `llm::ModelSlot`, one-ask `[CITE:n]` verbatim gate + `decompose::resolve_provision_label`, `hardware_detector`, `conversation` (chat_message), graph store (F007/F013 cross-reference + amendment edges)
**Storage**: Bundled PostgreSQL 17 + pgvector (`{app_data_dir}/pg/data`), single source of truth. Reuses `chat_message` + `conversation_summary` (migration 023). New **additive migration**: `pinned_case_fact`; depends on F013 amendment provenance (`footnote_refs` / `amended_by` edges)
**Testing**: `cargo test --lib` (unit: allocator, n_ctx resolver, citation gate, intent routing, pinned-fact extraction, amendment-chain ordering); integration acceptance test per P1 user story (no-truncation invariant, zero-confabulation gate)
**Target Platform**: Tauri desktop (Linux/Win/Mac), offline-first; **deployment surface = IBA GPU thin-client server (Spec 014)** — assistant logic exposed as commands that the server wraps over HTTP
**Project Type**: Single Tauri app (Rust backend `src-tauri/` + SolidJS frontend `src/`)
**Performance Goals**: Single-turn guidance p95 < 10 s (Tier 3/4, per constitution); whole-document/cross-document map-reduce capabilities are **inherently multi-pass** — progress-streamed, primarily targeting the GPU server (NOT bound by the per-turn budget — see Complexity Tracking)
**Constraints**: Offline; single resident LLM (Constitution III) — map-reduce reuses the one slot sequentially; `prompt ≤ n_ctx` guaranteed before generation; never chop a provision; citation-grade 100% accuracy
**Scale/Scope**: 7-doc Sindh pilot corpus → full Pakistan legal corpus; conversations of arbitrary length (eviction by summary + new-chat notice)

## Constitution Check

*GATE: evaluated against the 13 principles (I–VIII Core, IX–XIII Domain).*

| Principle | Status | Notes |
|---|---|---|
| I. Privacy-First, Offline-Always | ✅ PASS | All inference/storage local; no cloud calls. |
| II. Tiered Intelligence | ✅ PASS | Intent routing is deterministic (Tier 1) first; LLM (Tier 3/4) only for synthesis. Deterministic capabilities (amendment chain) never call the LLM for truth. |
| III. Single Model Slot | ✅ PASS | One resident LLM; map-reduce passes are **sequential** on the same slot, never concurrent. |
| IV. SQLite as Single Source of Truth | ⚠️ **VIOLATION (inherited)** | Uses bundled PostgreSQL (F011 re-platform). Pending constitution amendment / F011 merge gate. See Complexity Tracking. |
| V. Pending Actions Queue | ✅ PASS | Read/generate only; "offer-to-index" triggers indexing via the existing approval path. |
| VI. Persona-as-Config (TOML) | ✅ PASS (action) | Bureaucrat persona + per-capability prompts defined in TOML config, NOT hardcoded — explicit task. |
| VII. Hardware-Adaptive | ✅ PASS | Dynamic `n_ctx` clamps to RAM/VRAM; small windows degrade gracefully (summarize sooner, compact persona). |
| VIII. Incremental Delivery | ✅ PASS | P1 (US1–US3) is a usable MVP; later capabilities are additive on the same path. |
| IX. Pakistan Govt Legal Domain | ✅ PASS | Sindh/Pakistan legal corpus only. |
| X. Officer-First UX | ✅ PASS | General-answer-first, offer-to-tailor, plain-prose drafts. |
| XI. Citation Discipline | ✅ PASS | Every legal claim carries a verbatim citation via the gate; canonical citation format reused. |
| XII. Latest Text + Amendment Chain | ✅ PASS | Amendment chain is a first-class capability, deterministic from provenance. |
| XIII. Cross-Reference Graph First-Class | ✅ PASS | Amendment chain + conflict detection consume the graph, not re-derived. |

**Gate result**: PASS with one inherited, documented violation (IV) carried under the F011 merge gate.

## Project Structure

### Documentation (this feature)

```text
specs/015-bureaucrat-chat/
├── plan.md              # This file
├── research.md          # Phase 0 — resolved unknowns
├── data-model.md        # Phase 1 — entities + migration
├── quickstart.md        # Phase 1 — how to run/verify
├── contracts/           # Phase 1 — Tauri command + HTTP-shape contracts
│   └── assistant-commands.md
└── tasks.md             # Phase 2 — /sp.tasks (NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/
├── services/
│   ├── assistant/                 # NEW — the unified path
│   │   ├── mod.rs                 # turn orchestration (intent → capability → grounded answer)
│   │   ├── context_budget.rs      # NEW — real-token priority allocator (FR-010..FR-018)
│   │   ├── n_ctx.rs               # NEW — dynamic n_ctx from GGUF metadata + RAM/VRAM clamp + ECHO_N_CTX
│   │   ├── memory.rs              # NEW — pinned facts + summarize-old-turns eviction + fill notice
│   │   ├── capabilities/          # NEW — per-capability handlers (share invariants)
│   │   │   ├── guidance.rs        #   US1 (single-pass)
│   │   │   ├── draft.rs           #   US4 (single-pass, large gen reserve)
│   │   │   ├── summarize_doc.rs   #   US6 (map-reduce)
│   │   │   ├── compare.rs         #   US7 (cross-doc map-reduce)
│   │   │   ├── conflict.rs        #   US7 (cited candidates)
│   │   │   ├── amendment_chain.rs #   US8 (deterministic, graph; LLM narrates)
│   │   │   └── action_items.rs    #   US9 (map-reduce)
│   │   └── citation_gate.rs       # re-homed [CITE:n] verbatim gate + refuse-or-flag + offer-to-index
│   ├── query_router.rs            # EXTEND — allocator replaces hardcoded budgets; atomic provisions
│   ├── intent_router.rs           # EXTEND — capability classification/dispatch
│   ├── llm.rs                     # EXTEND — n_ctx from resolver; strip <think>; assert prompt ≤ n_ctx
│   └── conversation.rs            # EXTEND — pinned facts + summary
├── commands/
│   └── assistant.rs               # NEW — Tauri commands (HTTP-mappable for Spec 014)
├── db/pg/migrations/              # NEW additive migration — pinned_case_fact
└── config/personas/bureaucrat.toml # NEW — persona + per-capability prompts (Constitution VI)

src/ (frontend, SolidJS)
├── pages/ChatPage.tsx             # unified surface (replaces separate chat + drafting panes)
├── components/ContextFillNotice.tsx  # NEW — fill notice + new-chat-carry-facts
└── api/assistant.ts               # NEW — command bindings

RETIRE (remove from user-facing flow): services/drafting/{session,docset,gaps,ingest}.rs,
commands/drafting.rs, commands/one_ask.rs (keep extracted citation gate), template/slot schema usage.
```

**Structure Decision**: Single Tauri app. New `services/assistant/` subtree owns the unified path; `query_router`/`intent_router`/`llm` are extended in place (smallest viable diff). Drafting subsystem is retired but the citation gate is extracted, not deleted.

## Phasing (user-story aligned, P1 = MVP)

- **Phase A — Foundation (P1: US1+US2+US3)**: dynamic `n_ctx` resolver → context allocator (real tokens, priority, atomic provisions, `prompt ≤ window`) → re-home citation gate → single chat surface → guidance capability. Delivers a grounded, no-truncation assistant. *This is the MVP and de-risks the hardest invariant first.*
- **Phase B — Drafting + model (P2: US4+US5)**: draft capability (large gen reserve) + export; verify hot-swap + dynamic window across model sizes; `<think>` stripping.
- **Phase C — Whole-doc + amendment (P2: US6+US8)**: map-reduce orchestration (summarize a book); deterministic amendment chain from F013 provenance.
- **Phase D — Cross-doc (P2/P3: US7+US9)**: compare + conflict candidates; action items.
- **Phase E — Memory polish**: pinned-fact extraction, summarize-old-turns eviction, context-fill new-chat notice with fact carry-over.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **PostgreSQL instead of SQLite (Constitution IV + Tech-Stack table)** | Inherited from F011 re-platform (pgvector ANN at corpus scale; F012/F013 build on it). F015 reads the same store; switching back is out of scope. | Reverting to SQLite would regress F011–F013 and re-open the scaling problem PG solved. Carried under the existing F011 merge-gate / pending constitution amendment. |
| **Map-reduce capabilities exceed the per-turn latency budget** | Summarize-a-book / compare / conflict are inherently multi-pass over whole documents; cannot fit one window (the no-truncation guarantee forbids chopping). | A single-pass "summarize" would truncate the document — a direct violation of FR-014/SC-002. Multi-pass is progress-streamed and primarily targets the GPU server (Spec 014). |
| **New `services/assistant/` subtree (not folding into query_router)** | The unified path coordinates intent dispatch + allocator + memory + capabilities; co-locating keeps the invariants enforced in one place. | Spreading allocator/memory across existing files would scatter the "prompt ≤ window" guarantee and make truncation regressions easy to re-introduce. |
