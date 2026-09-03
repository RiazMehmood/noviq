# Implementation Plan: Pakistan Government Legal RAG

**Branch**: `007-legal-rag-pakistan` | **Date**: 2026-06-02 | **Spec**: [`spec.md`](./spec.md)
**Input**: Feature specification from `/specs/007-legal-rag-pakistan/spec.md`
**Design reference**: [`docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md`](../../docs/superpowers/specs/2026-06-02-legal-rag-pakistan-design.md) (architecture, data model, slice plan).
**Constitution**: [`.specify/memory/constitution.md`](../../.specify/memory/constitution.md) v1.1.0 (13 principles).

## Summary

Pivot EchoAI from a generic-domain RAG to a Pakistan government legal RAG. Officers drop PDFs of Acts/Ordinances/Rules/Regulations/Policies/Constitution. The fast path (existing 004/005 pipeline — no LLM) makes documents queryable within 60 s. On demand, an officer triggers Deep re-index, which runs the Mistral legal-IR pipeline, replaces existing chunks with Section-boundary-respecting chunks, and populates an amendment chain, a typed cross-reference graph, and procedure flows. A single chat window classifies queries into eight intents (Lookup, ListChildren, Summarize, Difference, CrossCompare, ActionItems, Timeline, Procedure) and dispatches to per-intent handlers; every answer carries an officer-grade citation. Default LLM tier is offline (Qwen 0.5B/3B, Mistral 7B); cloud is eval-only. Built as one feature branch in four slices (A, B, C, D) on top of a foundation Slice 0, each slice independently shippable with its own eval gate.

## Technical Context

**Language/Version**: Rust 1.87 (workspace) + TypeScript with SolidJS (frontend)
**Primary Dependencies**: tauri 2.0, llama-cpp-2 0.1.143 (with `llguidance` sub-feature for grammar-constrained sampling), sqlx 0.8 (SQLite), sqlite-vec (vec0), fastembed 4, async-trait, serde, serde_json, jsonschema, chrono. Reused from Feature 004/005/006: lopdf + pdftotext + tesseract OCR pipeline, FTS5 indexing, `progress_store`, `mistral_builder`.
**Storage**: SQLite via sqlx at `echo.db` (single source of truth — constitution §IV). JSON IR sidecars on disk at `{app_data_dir}/ir_cache/<sha>.json`; rasterised PDF pages at `{app_data_dir}/page_cache/<sha>/p<N>.png` (existing). Migration 020 is additive.
**Testing**: `cargo test` (unit + integration); golden-corpus tests on Sindh LG Act 2013, Sindh RTI Act 2017, Sindh LG Rules; 70-prompt officer eval set built in Slices B/C/D; final acceptance gate `cargo test --test legal_rag_v1_acceptance`.
**Target Platform**: Tauri desktop on Linux/Windows/Mac; CPU-class workstation (no required GPU); fully offline (no internet at runtime).
**Project Type**: Single project (Tauri shell + Rust backend in `src-tauri/`, SolidJS frontend in `src/`). Existing layout — no restructure.
**Performance Goals**: Fast-path index ≤ 60 s for ≤ 200-page PDF (SC-001); chat answers — fast-tier intents median ≤ 3 s, all others median ≤ 5 s / P95 ≤ 15 s on offline-mid (SC-011); Deep progress every 10 s, cancel within 5 s (SC-003); scope switch < 2 s (SC-009).
**Constraints**: Constitution §I (Privacy-First, Offline-Always); §III (Single Model Slot — at most one LLM resident in RAM); §IV (SQLite single source of truth); §XI (Citation Discipline — NON-NEGOTIABLE); §IX (Domain — Pakistan govt only); offline-default LLM tier (Qwen 0.5B/3B + Mistral 7B GGUF Q4_K_M); cloud (Groq/Cerebras) is eval-only with explicit per-query officer opt-in.
**Scale/Scope**: Up to 100 instruments / ~50,000 chunks per machine, single-officer desktop (SC-012, FR-033). Single-process Tauri; LAN multi-officer deferred to v2.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Verified against constitution v1.1.0 (all 13 principles).

### Core Principles (I–VIII)

| Principle | Status | Notes |
|---|---|---|
| I. Privacy-First, Offline-Always (NON-NEGOTIABLE) | ✅ PASS | FR-027 mandates offline default. FR-028/029 confine cloud LLMs to officer-initiated eval/regression — covered by I's "URLs explicitly provided by the user" carve-out. No telemetry, no automatic cloud calls. |
| II. Tiered Intelligence — Cheapest Tier First | ✅ PASS | Fast path (Tier 0–2: cache / FTS / ANN) services US1; LLM tier engaged only on Deep re-index trigger (officer-driven) or intent handlers that need it. Classifier uses rule-first then small-LLM tiebreak. |
| III. Single Model Slot — Bounded RAM Budget (NON-NEGOTIABLE) | ✅ PASS | Existing `ModelSlot` enforces single-resident model. Deep re-index uses Mistral 7B (worker tier); intent classifier uses Qwen 0.5B/3B (orchestrator tier); slot swaps follow constitution rules. No simultaneous loads. |
| IV. SQLite as Single Source of Truth | ✅ PASS | Migration 020 adds columns/tables on existing `echo.db`. No new DB files. JSON IR sidecars are caches, not authoritative state — DB is the truth. |
| V. Pending Actions Queue — Safety Gate (NON-NEGOTIABLE) | ✅ PASS | This feature performs no DANGEROUS tool calls. Deep re-index is the most "side-effect-y" action; it is officer-initiated, atomic, and cancellable. No bypass of the safety gate. |
| VI. Persona-as-Config — Agents Defined in TOML | ✅ PASS | Intent handlers are functional dispatch, not autonomous agents. No new TOML personas required; if one is later added (e.g., "officer chat" persona), it goes in the existing persona registry. |
| VII. Hardware-Adaptive Behavior | ✅ PASS | Existing tier detection covers RAM constraints. Deep re-index on a Tier 1 (<8 GB) machine MUST degrade gracefully — UI surfaces a "deep re-index requires ≥ 8 GB RAM" message; document remains fast-indexed. |
| VIII. Incremental Feature-App Delivery | ✅ PASS | This is Feature 007, independently shippable. US1/US2/US3 stack as Slices A/B/C → D so that any earlier slice merging alone leaves a usable product. |

### Domain & Product Principles (IX–XIII)

| Principle | Status | Notes |
|---|---|---|
| IX. Pakistan Government Legal Domain (NON-NEGOTIABLE) | ✅ PASS | Spec FR-001 enumerates the in-scope artifact types exactly as IX does. FR-032 explicitly excludes case law, foreign jurisdictions, private practice. |
| X. Officer-First UX | ✅ PASS | Spec primary user is the government officer / policy analyst (User Story P1/P2/P3); citizen and lawyer modes are not first-class in v1. |
| XI. Citation Discipline (NON-NEGOTIABLE) | ✅ PASS | FR-021 — every answer carries a citation. FR-022 — deep tier renders full hierarchical path. FR-023 — fast tier renders page/span + discloses limitation. FR-024 — single canonical format everywhere. The Slice 0 `services/citation/` module is the single source of truth. |
| XII. Latest Text + Amendment Chain | ✅ PASS | FR-010 covers amendment-event extraction. FR-030 explicitly forbids point-in-time queries in v1. |
| XIII. Cross-Reference Graph as First-Class | ✅ PASS | FR-011 covers cross-ref edge extraction. Cross-document intent handlers (Difference instrument-flavor, CrossCompare, Procedure) consume the graph rather than re-deriving relationships. |

**Gate result:** PASS on all 13 principles. No violations. **Complexity Tracking** section is intentionally empty (see below).

## Project Structure

### Documentation (this feature)

```text
specs/007-legal-rag-pakistan/
├── spec.md              # Feature spec (sp.specify + sp.clarify)
├── plan.md              # This file (sp.plan)
├── research.md          # Phase 0 — research findings (sp.plan)
├── data-model.md        # Phase 1 — entities, relationships, state (sp.plan)
├── quickstart.md        # Phase 1 — minimal officer walkthrough (sp.plan)
├── contracts/           # Phase 1 — Tauri command contracts + JSON schemas
│   ├── tauri-commands.md
│   ├── intent-classifier.schema.json
│   ├── legal-ir.schema.json
│   └── procedure.schema.json
├── checklists/
│   └── requirements.md  # sp.specify quality checklist (PASS)
└── tasks.md             # Phase 2 — task list (sp.tasks, not created by sp.plan)
```

### Source Code (repository root — existing single-project layout)

```text
src-tauri/
├── src/
│   ├── db/
│   │   ├── schema.rs                       # TARGET_SCHEMA_VERSION 15 → 16
│   │   └── migrations/
│   │       ├── 019_doc_ir_progress.sql     # Feature 006 (already on master)
│   │       └── 020_legal_ir.sql            # NEW — Slice 0
│   ├── services/
│   │   ├── citation/                       # NEW — Slice 0
│   │   │   ├── mod.rs
│   │   │   ├── types.rs                    # LegalCitation, PathSegment, AmendmentNote
│   │   │   └── formatter.rs                # officer-grade + short formatters
│   │   ├── legal_ir/                       # NEW — Slice A
│   │   │   ├── mod.rs
│   │   │   ├── types.rs                    # NodeKind enum
│   │   │   ├── schema.rs                   # llguidance JSON schemas
│   │   │   ├── prompts.rs                  # Mistral prompts
│   │   │   ├── amendment_parser.rs         # deterministic X[…] / [Subs. by …] pass
│   │   │   ├── xref_extractor.rs           # Slice C
│   │   │   ├── procedure_extractor.rs      # Slice D
│   │   │   ├── mistral_builder.rs          # reused from 006, re-parameterized
│   │   │   └── progress_store.rs           # reused from 006 as-is
│   │   ├── deep_reindex/                   # NEW — Slice A
│   │   │   ├── mod.rs                      # orchestrator: classify → IR build → replace chunks
│   │   │   └── chunks_replace.rs           # atomic delete + insert
│   │   ├── intent_router/                  # NEW — Slice B
│   │   │   ├── mod.rs
│   │   │   ├── classifier.rs               # rule-first + small-LLM tiebreak
│   │   │   ├── slots.rs                    # slot extraction (instrument, section, comparands)
│   │   │   └── handlers/
│   │   │       ├── lookup.rs
│   │   │       ├── list_children.rs
│   │   │       ├── summarize.rs
│   │   │       ├── action_items.rs
│   │   │       ├── difference.rs           # Slice C
│   │   │       ├── cross_compare.rs        # Slice C
│   │   │       ├── timeline.rs             # Slice C
│   │   │       ├── procedure.rs            # Slice D
│   │   │       └── fallback.rs
│   │   ├── indexer.rs                      # CHANGED — Tier check; route to legal_ir on deep
│   │   ├── llm.rs                          # CHANGED — explicit Tier param on every call
│   │   └── query_router.rs                 # CHANGED — delegate to intent_router
│   ├── commands/                           # Tauri commands
│   │   └── deep_reindex.rs                 # NEW — Slice A; emits progress events
│   └── lib.rs                              # CHANGED — register new commands
├── tests/
│   ├── citation_golden_test.rs             # NEW — Slice 0 (12 cases)
│   ├── migration_020_test.rs               # NEW — Slice 0
│   ├── legal_ir_slice_a_test.rs            # NEW — Slice A (re-index Sindh corpus)
│   ├── intent_classifier_test.rs           # NEW — Slice B (40-prompt set)
│   ├── xref_extractor_test.rs              # NEW — Slice C (100-edge sample)
│   ├── procedure_extractor_test.rs         # NEW — Slice D (15 procedure queries)
│   └── legal_rag_v1_acceptance.rs          # NEW — final acceptance gate
└── Cargo.toml                              # CHANGED — `llm` feature implies llama-cpp-2/llguidance

src/                                        # SolidJS frontend
├── components/
│   ├── DocActions.tsx                      # CHANGED — add "Deep re-index" button per doc
│   ├── ChatBubble.tsx                      # CHANGED — render citation block + "see also"
│   ├── DeepReindexProgress.tsx             # NEW — progress UI
│   └── FlowDiagram.tsx                     # NEW — procedure flow renderer (linear + expand)
└── pages/
    └── Chat.tsx                            # CHANGED — wire intent-router responses

tools/
└── migrate_generic_to_legal_ir.py          # NEW — Slice 0; one-shot sidecar migration

models/
├── ir_builder/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf   # already on disk (from 006)
├── classifier/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf      # already on disk
└── orchestrator/Qwen2.5-3B-Instruct-Q4_K_M.gguf      # already on disk
```

**Structure Decision**: Single Tauri project, existing layout. Add new modules under `src-tauri/src/services/{citation,legal_ir,deep_reindex,intent_router}/`. No restructure of existing code; all changes are additive at the module level. Frontend gains two new components (`DeepReindexProgress`, `FlowDiagram`) and modifies `DocActions`, `ChatBubble`, `Chat`. The migration 020 SQL file follows the existing additive-migration convention.

## Slice Plan

Four implementation slices on top of Slice 0 (Foundation). Each slice is an independently shippable PR with its own eval gate.

| Slice | Maps to user stories | Headline | Eval gate |
|---|---|---|---|
| **0 — Foundation** | — | Branch, constitution amendment, migration 020 (additive), `services::citation/` module + 12 golden tests, SpecKit scaffolding, sidecar migration tool. | Migration applies cleanly; citation_golden_test 12/12 GREEN; schema_wipe_test passes at v16. |
| **A — Fast path + Deep re-index + Legal IR + Amendment chain** | US1 (P1) fast-path basics + US2 (P2) deep upgrade + amendment chain | Officer can upload, ask basic queries on fast tier within 60 s; Deep re-index runs Mistral pipeline, replaces chunks atomically, persists amendment events. | Sindh LG Act + Sindh RTI Act + Sindh LG Rules re-index without error; ≥ 90% of top-level Sections captured; 100% of obvious amendment markers found; zero orphan nodes; SC-001 within budget. |
| **B — Intent router + 4 read-side intents** | US1 (Lookup, ListChildren, Summarize, ActionItems) | Chat understands what the officer is asking and dispatches. The "easy" intents ship. Citation rendering goes through `services::citation/`. | Build 40-prompt officer eval (10 per intent); offline-small ≥ 60% OK · offline-mid ≥ 75% · Groq baseline ≥ 90%. Citation accuracy ≥ 95% on lookups (SC-002, SC-004 partial). |
| **C — Cross-ref graph + 3 comparison intents** | US2 (Difference) + US3 (CrossCompare, Timeline) | `xref_extractor` writes `graph_edges`. Difference (amendment + instrument), CrossCompare (N-way), Timeline (amendment + thematic) handlers. "See also" surface lights up. | 30 new prompts (10 per intent); amendment-diff ≥ 80% OK on offline-mid; xref edge precision ≥ 85% on 100-edge human-graded sample. |
| **D — Procedure extraction + flow intent** | US3 (Procedure) | Procedural-section classifier + step extraction; cross-instrument stitching via `graph_edges`. `Procedure` handler + linear flow renderer with on-demand branching. | 15 procedure queries; ≥ 70% return a flow with all key steps + correct actors + correct citation per step; "show branches" works on at least 3 procedures with known conditional logic. |

### Final acceptance gate (end of Slice D = v1 ship)

A single `cargo test --test legal_rag_v1_acceptance` harness (modeled on the `constitution_v1` gate that already ships in the repo). Five gates:

1. Migration 020 runs clean on a copy of master `echo.db`.
2. Sindh LG Act + Sindh RTI Act + Sindh LG Rules re-index without error; node + edge + procedure counts match goldens within ±5%.
3. Citation formatter produces correct officer-grade strings for 12 golden cases.
4. Intent classifier ≥ 90% accuracy on 70-prompt eval (offline-mid).
5. End-to-end: 5 hand-picked officer queries (one per major intent including Procedure) return correct answer + correct citation on offline-mid.

Plus a separate, non-CI eval report archived under `specs/007-legal-rag-pakistan/eval/` covering offline-small / offline-mid / Groq baselines.

## Phase 0 — Research

See [`research.md`](./research.md). All Technical Context unknowns are resolved against the existing 006 engine + prior Feature 005 eval data.

## Phase 1 — Design & Contracts

- [`data-model.md`](./data-model.md) — Instrument, hierarchical node, amendment event, cross-ref edge, procedure, procedure step, document tier, chat query.
- [`contracts/tauri-commands.md`](./contracts/tauri-commands.md) — `deep_reindex_document`, `cancel_deep_reindex`, `chat_query`, scope-related commands.
- [`contracts/intent-classifier.schema.json`](./contracts/intent-classifier.schema.json) — grammar-constrained classifier output schema (8 intents + slots).
- [`contracts/legal-ir.schema.json`](./contracts/legal-ir.schema.json) — Mistral-generated legal IR JSON shape.
- [`contracts/procedure.schema.json`](./contracts/procedure.schema.json) — procedure step extraction schema.
- [`quickstart.md`](./quickstart.md) — minimal officer walkthrough: drop a Sindh PDF → ask first question → trigger Deep re-index → ask an amendment-aware question.

Agent context update: `.specify/scripts/bash/update-agent-context.sh claude` is invoked at end of this phase to refresh `CLAUDE.md` with the technologies referenced above.

## Constitution re-check (post-Phase 1)

To be re-run after `data-model.md` and `contracts/` are written. Expected result: PASS (no new gates triggered; the new entities are SQLite-resident per IV, the new commands honour the Pending Actions Queue rules per V (none are DANGEROUS), and citation contracts enforce XI).

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No violations. Section intentionally empty.
