# Implementation Plan: Document-Intelligence Production Overhaul

**Branch**: `012-doc-intelligence-production` | **Date**: 2026-06-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/012-doc-intelligence-production/spec.md`

## Summary

Make the offline, citation-grade legal-RAG drafting system production-ready by
fixing ingestion at the source (correct provision boundaries, resolved labels,
tables, no page-chrome masquerading as law) and making the drafting layer
structurally incapable of fabricating citations. Two non-negotiable guardrails
cut across every story: **G1** the verbatim-substring invariant (every number /
quote / reference / cell is a verbatim substring of the source digital text) and
**G2** the golden regression gate (a frozen real-corpus fixture set is the
release gate, diffed on every re-index).

Technical approach: a single trusted geometry source (`pdftotext -bbox`, poppler;
**no lopdf**) feeds an upstream chrome-strip (US1), a projection-profile table
engine (US5), and robust numbering-sequence heading detection (US3); the tree
build attaches only provision-classified blocks and persists resolved
`unit_label` + raw marker + provenance + confidence (US2); the drafting layer
composes from fixed slot+template structure with no LLM-authored prose, verified
by a decidable whitelist check (US4). The carried-over US8 work is **evolved**:
`parsers/table.rs` grows from the pairwise-gap heuristic into the projection-profile
engine; the downstream `is_boilerplate` flag is **superseded** by the upstream
chrome class (decision recorded in Phase 0).

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`); TypeScript + SolidJS (frontend)
**Primary Dependencies**: Tauri 2.0, `pdftotext` (poppler, system binary) for `-bbox`/`-layout` geometry & text, `regex_lite`, `fastembed` 4 (re-embed), `sqlx` 0.8 (postgres feature, runtime queries), `pgvector` casts, `serde`/`serde_json`, `tokio`. **NO lopdf for tables.** PaddleOCR/PP-Structure reserved for true scans (unused at launch — corpus has no scanned-only pages).
**Storage**: Bundled PostgreSQL 17 + pgvector (`{app_data_dir}/pg/data`), additive migrations under `src-tauri/src/db/pg/migrations/`. **Diverges from Constitution §IV (SQLite) — see Complexity Tracking; pending amendment / merge gate (F011).**
**Testing**: `cargo test --lib` for pure logic (G2 golden fixtures = frozen real-corpus cases); one attended live re-index per wave as the acceptance gate (chrome excluded, labels resolved, tables coherent, drafting deterministic).
**Target Platform**: Linux desktop (Tauri), NTFS-mounted working volume (slow cold builds ~30 min — batch one re-index per wave).
**Project Type**: Single desktop app (Rust backend + SolidJS frontend). Ingestion + drafting are backend services.
**Performance Goals**: No regression to p95 query latency (<2s Tier 2, <10s Tier 3/4). One full re-embed budgeted per re-index wave. Chrome-strip / table reflow are index-time (offline), not on the query path.
**Constraints**: Offline-only (Principle I); G1 verbatim-substring invariant on every emitted token; G2 golden gate before any re-index is trusted; conservative keep-bias (a missed table or un-stripped chrome is acceptable; a scrambled cell or a stripped real provision is not).
**Scale/Scope**: ~10 source documents, ~4–7k chunks live (prod PG). Five user stories across three re-index waves (A: US1→US2→US3; B: US4 parallel no re-index; C: US5).

## Constitution Check

*GATE: re-evaluated after Phase 1 design — still passing.*

| # | Principle | Verdict | Notes |
|---|-----------|---------|-------|
| I | Privacy-First, Offline-Always | ✅ PASS | All extraction (poppler), embedding (fastembed/ONNX), inference local. No cloud. |
| II | Tiered Intelligence | ✅ PASS | US4 *removes* LLM prose authoring → drafting leans on cheaper deterministic tiers; no new escalation. |
| III | Single Model Slot | ✅ PASS | US4 drops the 7B from the prose path; still ≤1 resident model. |
| IV | SQLite Single Source of Truth | ⚠️ DIVERGENCE | App runs bundled Postgres+pgvector (F011 port). Pre-existing, not introduced here. Pending constitution amendment / merge gate. See Complexity Tracking. |
| V | Pending Actions Queue | ✅ N/A | Ingestion/drafting add no DANGEROUS tool calls. |
| VI | Persona-as-Config (TOML) | ✅ N/A | No agent/persona change. |
| VII | Hardware-Adaptive | ✅ PASS | Index-time CPU work; graceful, no new tier gates. |
| VIII | Incremental Feature-App Delivery | ✅ PASS | Waves A/B/C each independently shippable; US4 has no re-index dependency. |
| IX | Pakistan Govt Legal Domain | ✅ PASS | Core target; no scope broadening. |
| X | Officer-First UX | ✅ PASS | Deterministic secretariat-register drafting optimizes the officer flow. |
| XI | Citation Discipline | ✅ PASS (strengthens) | US2 resolved hierarchical labels + G1 make citations structurally correct; refuse-or-flag over confident-wrong. |
| XII | Latest Text + Amendment Chain | ✅ PASS | US2 keeps raw marker + provenance; unparsed markers tolerated as flags, not failures. |
| XIII | Cross-Reference Graph First-Class | ✅ DEFERRED | KG is an explicit spec non-goal (Feature 013+); clean ingestion here is its hard prerequisite. No regression. |

**Result:** PASS with one documented divergence (§IV, pre-existing). No new violations.

## Project Structure

### Documentation (this feature)

```text
specs/012-doc-intelligence-production/
├── spec.md              # committed
├── plan.md              # this file
├── research.md          # Phase 0 — decisions (chrome signals, projection profiles, US8-carryover disposition)
├── data-model.md        # Phase 1 — chunk schema additions (resolved label, raw marker, provenance, confidence, block class)
├── contracts/           # Phase 1 — internal module contracts + invariants (G1/G2)
│   ├── chrome-strip.md
│   ├── table-engine.md
│   ├── heading-detection.md
│   ├── label-provenance.md
│   └── deterministic-drafting.md
├── quickstart.md        # Phase 1 — run a wave: build → golden gate → re-index → verify
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root) — touched paths

```text
src-tauri/src/services/
├── parsers/
│   ├── pdf.rs                  # bbox IO shell; wire chrome-strip + table engine into Tier-1 (US1, US5)
│   ├── table.rs                # EVOLVE pairwise-gap → projection-profile + cross-page stitch (US5)
│   ├── boilerplate.rs          # SUPERSEDE by upstream chrome class (US1); disposition in research.md
│   └── chrome.rs               # NEW — page-chrome classification (US1)
├── heading_detector/
│   └── regex_pass.rs           # numbering-sequence prior + boundary-stop + confidence (US3); year-rule STAYS
├── structure_parser.rs         # tree build attaches provision blocks only; label-content split action (US3);
│                               #   resolved unit_label + raw marker + provenance + confidence (US2)
├── structured_importer.rs      # persist provenance/confidence/block-class; kill silent synthetic-id fallback → flag (US2)
└── drafting/one_ask/
    ├── compose.rs              # deterministic slot fill; no LLM prose (US4)
    ├── skeleton.rs             # slot+template frames (US4)
    └── verify.rs               # decidable whitelist check (US4, G1)
src-tauri/src/db/pg/migrations/
├── 002_chunks_boilerplate.sql  # carried-over (decide keep/repurpose in research.md)
└── 003_chunks_provenance.sql   # NEW additive — block_class, raw_marker, page, char_span, boundary_confidence (US2)
```

**Structure Decision**: Single desktop app. All work lands in existing backend service modules under `src-tauri/src/services/{parsers,heading_detector,drafting}` plus `structure_parser.rs`/`structured_importer.rs`, with one new `parsers/chrome.rs` and one additive migration. No new crate, no frontend change at launch (drafting output shape unchanged; provenance is internal/auditable).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Storage on PostgreSQL+pgvector, not SQLite (Constitution §IV) | Pre-existing from Feature 011 (vector-search scale, pgvector). This feature does not reintroduce or deepen the divergence — it only adds additive migrations to the existing PG schema. | Reverting to SQLite is out of scope and would regress F011. Tracked as a pending constitution amendment + merge gate (recorded in MEMORY). |
| New `parsers/chrome.rs` module (vs extending `boilerplate.rs`) | Chrome-strip must run **upstream of tree-build** and classify blocks (provision vs chrome) before labels are assigned — a different stage and contract than the downstream `is_boilerplate` flag. | Keeping the flag-at-persist approach was proven insufficient live (flagged 0/4277; junk inherits provision labels). The defect is upstream; the fix must be too. |
```
