# Implementation Plan: LLM PDF → Hierarchical JSON Importer

**Branch**: `006-llm-pdf-importer` | **Date**: 2026-05-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/006-llm-pdf-importer/spec.md`

## Summary

Route every imported PDF through a local 7B language model that emits the same hierarchical IR JSON the constitution path already consumes. The IR feeds the existing `graph_builder` and `indexer`, so downstream retrieval is unchanged. Partial-success ingest: confident nodes are searchable immediately; low-confidence nodes appear in a split-screen repair UI (rendered PDF page + JSON editor). User repairs are sticky across re-runs. Cache is sha256-keyed on disk, with a SQLite index table and a 7-day orphan sweep. When the GGUF is missing the system falls back to Feature 005's rule-based parser. Cloud LLMs are explicitly excluded in v1 (Constitution Principle I).

Technical approach: define `trait IrBuilder` with one impl `MistralIrBuilder` using llama-cpp-2's **llguidance constrained sampler** against a JSON-schema grammar; validator runs schema-then-semantic; cache lives at `{app_data_dir}/ir_cache/<sha>.json` indexed by a new `ir_cache_index` SQLite table (migration 018). Repair UI rasterizes pages via pdftoppm (already present from Feature 004). Reuses Feature 005's heading-detection pre-pass for chunk boundary selection.

## Technical Context

**Language/Version**: Rust 1.87 (workspace), TypeScript with SolidJS for the frontend
**Primary Dependencies**: tauri 2.0, llama-cpp-2 0.1.143 (llguidance constrained sampling), sqlx 0.8, fastembed 4, async-trait, serde, serde_json, jsonschema (new), chrono
**Storage**: SQLite via sqlx at `echo.db` (single source of truth, additive migration 018); JSON payloads on disk at `{app_data_dir}/ir_cache/<sha>.json`; rasterized PDF pages at `{app_data_dir}/page_cache/<sha>/p<N>.png`
**Testing**: cargo test for unit + integration; new test target `llm_ir_importer_acceptance` (model-free, frozen DocIr stub) for P1+P2+P3 gate; manual run against `models/mistral-7b-instruct-v0.3.Q4_K_M.gguf` produces the golden record at `specs/006-llm-pdf-importer/golden/`
**Target Platform**: Linux + Windows desktop via Tauri 2; CPU-only llama.cpp (GPU optional via existing `priority_model_slot` switch)
**Project Type**: Single project — Rust backend + SolidJS frontend already collocated under one repo
**Performance Goals**: 7B Q4_K_M ≥ 6 tokens/sec on Tier-3 hardware; per-document conversion budget driven by token count (~80 sections × ~250 output tokens × 6 tok/s ≈ 55 min worst case — acceptable for one-time per document); cached-reimport <5s end-to-end (SC-006); repair-save partial re-index <10s (SC-005)
**Constraints**: Constitution Principle III bounds RAM — Mistral 7B occupies the Tier-4 worker slot, demand-loaded, evicted after 90s idle; orchestrator 3B remains warm; importer must NEVER hold the worker resident across documents (release between docs); offline-only (Principle I); additive-only migrations
**Scale/Scope**: Target corpus ~1000 mixed structured PDFs / user; cached IR storage <500 MB at that scale (SC-007); validator handles documents up to 5000 nodes; repair UI navigates flagged-node sets of typical size 1–20 per document

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design — see end of section.*

| # | Principle | Status | Notes |
|---|---|---|---|
| I | Privacy-First, Offline-Always | PASS | v1 explicitly forbids cloud LLM in importer; only local Mistral 7B; cache stays on disk; no telemetry. Out-of-scope in spec backs this. |
| II | Tiered Intelligence — Cheapest Tier First | PASS | Importer is per-document, not per-query. It is gated by Tier-0 cache (sha256 reuse, FR-019/022) before invoking Tier-4 worker. Chat continues to follow the standard escalation. |
| III | Single Model Slot — Bounded RAM | JUSTIFIED | Mistral 7B as Tier-4 worker is correct, BUT the importer monopolizes the worker slot during a conversion. While converting, chat queries cannot escalate to the worker tier. Acceptable because (a) conversion is user-initiated and progress-visible, (b) chat at Tier 3 continues to work, (c) worker is released between docs. Logged in Complexity Tracking. |
| IV | SQLite as Single Source of Truth | PASS | `ir_cache_index`, `ir_repair_log`, and new `sources` columns live in `echo.db` via additive migration 018. JSON payloads on disk are derived/cached artefacts (analogous to FTS5 indexes), not authoritative state. |
| V | Pending Actions Queue — Safety Gate | PASS | No DANGEROUS tools introduced. The repair-save command is CAUTIOUS at most (writes inside app data dir; rewrites cached JSON; re-embeds chunks for an existing source). |
| VI | Persona-as-Config — Agents in TOML | n/a | This feature is not an agent. No persona changes. |
| VII | Hardware-Adaptive Behavior | PASS | `MistralIrBuilder::available()` returns false on hardware tier < 3 (or when load fails OOM); importer falls back to rule-based parser with a one-time notification. No silent failure. |
| VIII | Incremental Feature-App Delivery | PASS | Feature 006 ships P1 alone as a usable MVP (auto-conversion). P2 (repair UI) and P3 (cache lifecycle) are independently shippable. |

**Re-check after Phase 1**: all gates still pass; no new violations introduced by Phase 1 contracts.

## Project Structure

### Documentation (this feature)

```text
specs/006-llm-pdf-importer/
├── spec.md                         # Phase −1 (/sp.specify output)
├── plan.md                         # This file (/sp.plan output)
├── research.md                     # Phase 0 — technology decisions
├── data-model.md                   # Phase 1 — IR types, SQLite schema, lifecycle
├── contracts/
│   ├── ir_builder_trait.md         # IrBuilder trait + dispatch order
│   ├── tauri_commands.md           # Frontend ↔ backend wire format
│   └── ir_json_schema.md           # DocIr JSON-schema (llguidance + validator)
├── quickstart.md                   # Dev quickstart
├── checklists/
│   └── requirements.md             # Spec quality checklist (from /sp.specify)
└── tasks.md                        # Phase 2 — produced by /sp.tasks (NOT in this run)
```

### Source Code (repository root)

New code lives under `src-tauri/src/services/doc_ir/`. Existing files touched minimally — only the indexer dispatch hook and the doc_profiles routing entry get edits.

```text
src-tauri/
├── Cargo.toml                                          # ✎ add jsonschema, async-trait
├── src/
│   ├── db/
│   │   └── migrations/
│   │       └── 018_llm_ir_importer.sql                 # ✚ new (data-model.md §3)
│   ├── services/
│   │   ├── doc_ir/                                     # ✚ new module — owns IR types + builder + validator + cache
│   │   │   ├── mod.rs
│   │   │   ├── types.rs                                # DocIr, StructureNode, NodeKind, QualityFlag, Annotation
│   │   │   ├── schema.json                             # JSON-schema (single source of truth)
│   │   │   ├── builder.rs                              # trait IrBuilder + BuilderInput/Output/Error
│   │   │   ├── mistral_builder.rs                      # MistralIrBuilder impl
│   │   │   ├── grammar.rs                              # JSON-schema → llguidance binding
│   │   │   ├── chunker.rs                              # heading-boundary + token-budget pre-chunker
│   │   │   ├── prompts.rs                              # versioned system prompts per doc_type
│   │   │   ├── validator.rs                            # schema + semantic passes; ValidationIssue
│   │   │   ├── cache.rs                                # disk JSON + ir_cache_index ops + orphan sweep
│   │   │   ├── repair.rs                               # apply NodeEdit + partial re-index
│   │   │   ├── retry.rs                                # 3-chunk / 2-doc retry loop
│   │   │   ├── progress.rs                             # ProgressSink + Tauri event emission
│   │   │   └── converter_from_constitution.rs         # ConstitutionIr → DocIr bridge
│   │   ├── indexer.rs                                  # ✎ add LLM IR dispatch branch in do_index
│   │   ├── structured_importer.rs                      # unchanged (fallback path)
│   │   ├── constitution_ir.rs                          # unchanged (constitution path)
│   │   ├── scheduler.rs                                # ✎ schedule daily orphan_sweep tick
│   │   └── parsers/                                    # unchanged
│   ├── commands/
│   │   ├── mod.rs                                      # ✎ register new commands
│   │   ├── importer.rs                                 # ✚ get_doc_ir, get_low_confidence_nodes,
│   │   │                                               #   render_pdf_page, save_repair,
│   │   │                                               #   list_docs_needing_review, cancel_import,
│   │   │                                               #   ir_cache_stats, force_orphan_sweep
│   │   └── …
│   └── lib.rs                                          # ✎ register importer commands + sweep tick
└── tests/
    ├── llm_ir_importer_acceptance.rs                   # ✚ P1+P2+P3 gate (model-free, frozen stub)
    ├── ir_validator.rs                                 # ✚ validator unit tests
    └── ir_cache_lifecycle.rs                           # ✚ cache state transitions + sweep

src/                                                    # SolidJS frontend
├── components/
│   ├── DocCard/
│   │   └── ReviewBadge.tsx                             # ✚ "N sections need review"
│   └── RepairView/                                     # ✚ split-screen repair UI
│       ├── RepairView.tsx                              # container, prev/next, save
│       ├── PdfPagePane.tsx                             # left pane — img src=render_pdf_page output
│       └── IrEditorPane.tsx                            # right pane — JSON editor + live validation
├── hooks/
│   └── useImporterProgress.ts                          # ✚ subscribes to importer://progress|done
├── services/
│   └── importer.ts                                     # ✚ typed wrappers around the new invoke() calls
└── routes/
    └── docs/[id]/repair.tsx                            # ✚ route for the repair view

models/
└── mistral-7b-instruct-v0.3.Q4_K_M.gguf                # ◯ user-supplied; absent → fallback path

specs/006-llm-pdf-importer/
└── golden/
    └── sindh_csa_1973.docir.json                       # ✚ manual capture, used by acceptance gate
```

**Structure Decision**: Single project (Rust backend + SolidJS frontend already collocated). New code is concentrated in one new module `src-tauri/src/services/doc_ir/` so the feature has a single review surface. Existing services (`indexer.rs`, `scheduler.rs`, command registration) get the minimum dispatch + wiring edits. Frontend code lives under `src/components/RepairView/` plus a new route — no global frontend refactors.

## Phase 0 — Output

Resolved in `research.md`:

- R1 constrained-decoding library → llguidance JSON-schema
- R2 model + tier → Mistral 7B Q4_K_M as demand-loaded Tier-4 worker
- R3 schema versioning → new `DocIr` type, converter from `ConstitutionIr`
- R4 validator architecture → schema + semantic two-pass
- R5 chunking → heading-boundary + 2400-token budget
- R6 cache storage → disk JSON + SQLite index
- R7 page rasterization → pdftoppm at 150 DPI
- R8 partial re-index → scoped node-set re-embed
- R9 trait + fallback dispatch → ordered try-list

No `[NEEDS CLARIFICATION]` remains.

## Phase 1 — Output

- `data-model.md` — Rust IR types, JSON envelope, SQLite migration 018, cache lifecycle, validation issue codes
- `contracts/ir_builder_trait.md` — `IrBuilder` trait + dispatch order in `indexer::do_index`
- `contracts/tauri_commands.md` — frontend ↔ backend wire format (events + commands)
- `contracts/ir_json_schema.md` — JSON schema used by llguidance and validator
- `quickstart.md` — dev quickstart, smoke tests, tear-down

Agent context update: see "Agent Context" section below.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Mistral 7B monopolizes the Tier-4 worker slot while a PDF is converting (Principle III edge case) | The IR builder needs 7B-class quality on legal text; 3B regresses structural fidelity below the SC-002/SC-003 thresholds (memory `project_echoai_22prompt_eval` proved 3B regressed with K=12→6). | Reusing the 3B orchestrator would skip the Tier-4 load but produce poorer IR — directly contradicts the feature's value prop. Splitting the conversion across two smaller models would still need one worker slot; net no win. |
| Two parallel IR types (`ConstitutionIr` + new `DocIr`) live in the codebase for the duration of this feature | Smallest-viable-diff: refactoring `ConstitutionIr` into a typed view over `DocIr` *now* would re-open Feature 003's acceptance gate (5/5 pass at ded893c5). | The converter in `converter_from_constitution.rs` keeps the path open to consolidate later. A v3 follow-up task can complete the merger once `DocIr` has proven itself in production. |
| JSON payloads live on disk instead of inside SQLite (Principle IV edge) | Cached IR is a derived artefact (regenerable from PDF + LLM), not authoritative state. Storing the **index** in SQLite preserves the principle's intent. Files-on-disk match how `constitution_ir.json` is already shipped, and keep payloads inspectable by `cat` / `jq` during development. | BLOB columns in SQLite would balloon row sizes (typical ~200 KB JSON each), make debugging awkward, and offer no transactional benefit since the index row + JSON file are written together via a small wrapper. |

## Agent Context

This plan adds the following technologies to the project's active stack — captured for the agent-context update:

- **llama-cpp-2 llguidance constrained sampler** (JSON-schema mode) — used for IR generation
- **jsonschema crate** — schema-pass validator
- **async-trait crate** — `IrBuilder` trait object dispatch
- **pdftoppm CLI** — already present from Feature 004, newly used by `render_pdf_page` command
- **Mistral 7B Instruct v0.3 GGUF (Q4_K_M)** — new model artefact, optional at runtime

Run `.specify/scripts/bash/update-agent-context.sh claude` to persist these into the agent-context file (preserves manual additions between markers).
