# Implementation Plan: Fast-Tier Ingestion Hardening (Citation-Grade)

**Branch**: `013-ingestion-hardening` | **Date**: 2026-06-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/013-ingestion-hardening/spec.md`

## Summary

Harden the **deterministic fast-tier ingestion pipeline** so that the labelled chunks every
downstream stage relies on faithfully represent a digital legal/government PDF's structure and text.
The Iteration-1 judge audit of `fundamental_rights.pdf` (Constitution of Pakistan, Part II,
Articles 7–40) surfaced 12 defect classes (D1–D12) spanning lost/mislabelled provisions, footnote &
page-number contamination, altered wording, NULL page numbers, and clause/amendment boundary errors.

**Technical approach** — evolve, don't rewrite. The codebase already separates **structure**
(`structure_parser` `DocNode` tree + `chunks.unit_type`/`parent_chunk_id`) from **vocabulary**
(`doc_profiles` `citation_unit`/`citation_fmt`) and already carries a `label_status='unresolved'`
state (migration `003_chunks_provenance.sql`). We sharpen this two-pass spine: **structure first**
(identifier + `NodeLevel`, no citation word), **vocabulary later** with confidence layering →
`unresolved` on conflict. A **new geometry-zoning step** (using existing `pdftotext -bbox` /
`WordBox` geometry) isolates page-numbers and footnotes *before* heading detection so the structural
parser sees clean body text. US5 captures amendment & cross-reference links into the **existing**
`graph_edges` / `amendment_events` store — capture only, no graph querying.

Work is sequenced in a **LOCKED 5-phase order** (clean signal → structure → vocabulary → boundaries),
one re-index + frozen-fixture diff per phase. No LLM anywhere in this pipeline (fast tier is
deterministic).

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`); deterministic — no model/LLM
**Primary Dependencies**: `pdftotext` (poppler, `-bbox` + `-layout` geometry), `regex_lite`, `sqlx`
0.8 (postgres feature, runtime queries), `serde`/`serde_json`, `tokio`. **No** `llama-cpp-2`,
`fastembed`, or `lopdf` involvement in the structure/label/text path. PaddleOCR explicitly out of scope.
**Storage**: Bundled PostgreSQL 17 + pgvector (`{app_data_dir}/pg/data`); existing `chunks`,
`graph_nodes`, `graph_edges`, `amendment_events` tables. Schema additions (if any) are additive
migrations under `src-tauri/src/db/pg/migrations/`.
**Testing**: `cargo test --lib` (unit: regex_pass, chrome, text_normalizer, doc_profiles,
structure_parser); a new frozen-fixture acceptance test for the 177-chunk Constitution Part II sample
plus the prose & table control docs.
**Target Platform**: Linux/desktop (Tauri 2.0), offline.
**Project Type**: Single project (Rust backend services); no frontend surface in this feature.
**Performance Goals**: Fast tier must remain fast (no 7B load); per-document re-index of the test
set in seconds, not minutes. Within constitution p95 budgets.
**Constraints**: **Verbatim-substring invariant** (every stored provision's text, whitespace-
normalized, is a substring of the source — never synthesized). **Flag-don't-fabricate**
(`label_status='unresolved'`, `citation_unit=NULL` over confident-wrong). Deterministic fast tier.
Additive migrations only. Smallest viable diff per phase.
**Scale/Scope**: 3 controlled test documents (simple prose, small table, the 177-chunk digital law
PDF frozen as the primary regression fixture). Generic across constitution/act/rules/manual/generic
profiles — **no Constitution-specific hardcode**.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Compliance |
|---|---|
| I. Privacy-First, Offline-Always | ✅ Pure-local deterministic parsing; no network, no telemetry. |
| II. Tiered Intelligence | ✅ Fast tier (regex/geometry/sequence) only; never escalates to a model. FR-013. |
| III. Single Model Slot | ✅ No model is loaded by this pipeline; RAM budget untouched. |
| IV. SQLite as Single Source of Truth | ⚠️ **Pre-existing divergence** — project runs on bundled PostgreSQL+pgvector (inherited from F011/F012; merge-gate amendment pending). This feature adds no new DB engine; uses existing `chunks`/graph tables. Tracked in Complexity. |
| V. Pending Actions Queue | ✅ N/A — ingestion is not a DANGEROUS tool call surface. |
| VI. Persona-as-Config | ✅ N/A — no agent/tool behaviour changes. |
| VII. Hardware-Adaptive | ✅ Deterministic path runs on every tier; no tier-gated features added. |
| VIII. Incremental Feature-App Delivery | ✅ Phased, each phase independently shippable + regression-locked. |
| IX. Pakistan Govt Legal Domain | ✅ Directly serves Constitution/Acts/Rules ingestion; no scope broadening. |
| X. Officer-First UX | ✅ Citation-grade correctness is the officer's core need; no UX regressions. |
| XI. Citation Discipline | ✅ Strengthens it — correct identifier/word/page, `unresolved` instead of wrong word. Fast-tier page/span citations honoured. |
| XII. Latest Text + Amendment Chain | ✅ US5 captures amendment events (`amendment_events`) verbatim or `unresolved`; tolerates unparsed markers. No point-in-time reconstruction. |
| XIII. Cross-Reference Graph as First-Class | ✅ US5 *populates* `graph_edges` (`cross_reference`/`amended_by`); graph *querying* explicitly deferred (Non-Goal). |

**Gate result**: PASS (one pre-existing, already-tracked IV divergence; no new violations).

## Project Structure

### Documentation (this feature)

```text
specs/013-ingestion-hardening/
├── plan.md              # This file (/sp.plan output)
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (internal stage interfaces)
│   └── ingestion-stages.md
├── checklists/
│   └── requirements.md  # (existing — spec quality gate)
└── tasks.md             # /sp.tasks output (NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/services/
├── text_normalizer.rs            # P0/D9 — audit JOIN_TARGETS; no cross-space word fusion
├── structured_importer.rs        # P0/D12 — thread page_offsets into persist; persist_footnote_chunks (P1)
├── structure_parser.rs           # P0/D12 page_number; P2 tree (chapter, catchline-merge, unit_type); P4 clause/bracket boundaries
├── parsers/
│   ├── chrome.rs                 # P1/D5,D6 — geometry zoning: page-number + footnote bands (Block.y_band)
│   ├── pdf.rs                    # geometry source (pdftotext -bbox / WordBox) — read-only reuse
│   └── table.rs                  # WordBox struct (reused, untouched)
├── heading_detector/
│   └── regex_pass.rs             # P2/D2,D3 — bare-number + alpha-suffix articles, CHAPTER nodes, sequence validation
├── doc_profiles/
│   ├── mod.rs                    # P3/D1 — vocabulary resolver entry (confidence layering)
│   ├── generic.rs                # P3/D1 — stop minting "Section {num}" at detection; defer word to resolver
│   └── constitution.rs           # P3/D1 — profile evidence
├── graph_builder.rs              # US5/FR-015 — cross_reference edges (article_refs already present)
└── legal_ir/
    └── amendment_parser.rs       # US5/FR-014 — amendment events from isolated footnotes → amendment_events

src-tauri/src/db/pg/migrations/
└── 004_*.sql                     # (only if needed) footnote anchor column / block_class='footnote'

src-tauri/tests/
└── ingestion_hardening_acceptance.rs   # frozen-fixture regression (FR-012, SC-001…SC-009)
```

**Structure Decision**: Single Rust-services project. This feature touches the existing deterministic
ingestion path only; no new crates, no frontend. Each defect maps to a named function in an existing
module (see `~/Desktop/ingestion_hardening_plan.md` + `LAW_DOC_JUDGE_AUDIT.md` for the per-defect code
map). The frozen 177-chunk fixture lives under `src-tauri/tests/`.

## Phased Delivery (LOCKED order)

Each phase: implement → `cargo test --lib` green → re-index the 3-doc test folder (fast path) →
re-dump chunks → diff vs frozen fixture → confirm the phase's target defects gone + no prior phase
regressed → update `MODULAR_IMPROVEMENT_LOG.md` + `Noviq_Ingestion_Tracker.xlsx`. Then next phase.

| Phase | Defects | User Stories | Goal |
|---|---|---|---|
| **P0** | D9, D12 | US2, US3 | Stop corrupting quotes (JOIN_TARGETS audit); populate `page_number` (thread `page_offsets` into `persist_tree`). Isolated, orthogonal. |
| **P1** | D5, D6 | US2, US3 | Geometry zoning (`Block.y_band` + cross-page repetition): footer page-numbers → chrome; footnote band → `block_class='footnote'`, retrievable, anchored-by-marker or `unresolved`. **Never text-regex** (legal text is full of bare integers). |
| **P2** | D2, D3, D4, D10 | US1 | Structural tree: bare-number + alpha-suffix article detection (`7`,`10A`,`19A`,`25A`) guarded by sequence validation; `CHAPTER N` → real Chapter nodes; catchline↔body merge into one Article node (title + body, spans preserved); correct `unit_type`. Identifier + level only — **no citation word yet**. 25/25A: flag, never auto-correct. |
| **P3** | D1 | US1 | Vocabulary resolver: confidence layering (profile → internal linguistic evidence → user override → `unresolved`). Stop `generic.rs` minting `Section {num}` at detection; bind the word at this layer. |
| **P4** | D7, D8 | US4 | Clause/sub-clause boundary splitting (`(a)` must not swallow `(b)`); amendment brackets `[ … ]` preserved verbatim (don't break tokenizer on `[`; only separate the footnote-reference digit). |
| **US5** | (FR-014/15) | US5 (P3) | After P1 (footnotes isolated) + P4 (brackets handled): capture amendment events (`amendment_events`) and cross-references (`graph_edges`) into the existing graph store, verbatim or `unresolved`. |

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Storage on PostgreSQL+pgvector, not SQLite (Principle IV) | Pre-existing from F011/F012; this feature reuses the live `chunks`/`graph_edges`/`amendment_events` tables. | Re-porting to SQLite for one ingestion feature is out of scope and would regress the running corpus. Divergence is already logged for the F011 merge-gate constitution amendment; no new deviation introduced here. |
