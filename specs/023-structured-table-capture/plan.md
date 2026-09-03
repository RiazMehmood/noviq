# Implementation Plan: General Structured-Table Capture at Ingestion

**Branch**: `023-structured-table-capture` | **Date**: 2026-07-12 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/023-structured-table-capture/spec.md`

## Summary

PDF tables are already detected by the pdfplumber ingestion sidecar as structured
`SidecarTable{header, rows}`, but that structure is consumed only to *linearise* the table into prose text
and is then discarded; the linearised text flows through the legal clause parser, which shreds a table into
hundreds of provision/clause micro-chunks (RoB Schedule-I → 292 chunks), and the downstream `table_row`
`block_class` tag over-tags ordinary enumerated clauses. Consequence: no single unit holds a whole table, so
counting ("how many departments" → 12–14 not 39), listing, and whole-document summaries are all incomplete.

**Approach**: carry the authoritative detected-table structure from the reflow stage through to unit
creation, and emit — per detected table — **one whole-table unit** (header + all rows) and **one unit per
row** (row + header context), typed distinctly so they **bypass the clause parser** and are never shredded;
mirror them into the document graph as a **table node with row-child nodes**. Deterministic, no LLM, reusing
the existing sidecar. Graceful fallback to today's behaviour when no table is detected or the sidecar is
unavailable. Additive migration only. First validation: RoB Schedule-I → 39.

## Technical Context

**Language/Version**: Rust 1.87 (workspace `echo-ai`, `src-tauri`)
**Primary Dependencies**: existing only — pdfplumber ingestion sidecar (`services/parsers/table_sidecar.rs`),
`services/parsers/table.rs` (reflow/linearise), `services/parsers/pdf.rs` (extraction entry),
`services/structure_parser.rs` + `services/structured_importer.rs` + `services/indexer.rs` (unit creation),
`services/graph_builder.rs` (graph nodes/edges), `sqlx` 0.8; `regex_lite`. No new crates.
**Storage**: bundled PostgreSQL + pgvector (`echo` db) — the live store (ADR-0015; diverges from Constitution
§IV, see Complexity Tracking). New **additive migration** widens the `chunks.unit_type` and
`graph_nodes.node_type` CHECK vocabularies to include table unit/node kinds; no column/table drops.
**Testing**: `cargo test --lib` on the VM (`--no-default-features --features llm,server,cuda`); unit tests
for table-unit emission + golden tests for reflow/marker changes; live re-index of RoB as the acceptance check.
**Target Platform**: Linux headless server (`noviq-server`) — the production ingestion path.
**Project Type**: single (Rust workspace `src-tauri`).
**Performance Goals**: within the existing table-extraction budget (`TIMEOUT = 300s` per doc, `table_sidecar`);
adds only deterministic string assembly per table — negligible.
**Constraints**: deterministic, NO LLM (FR-007); must not regress prose-only docs (FR-008, SC-004); graceful
fallback when sidecar absent (FR-008); scale to thousands of docs, no per-document tuning (FR-006).
**Scale/Scope**: ~14 live docs today, designed for thousands; tables from a handful to hundreds of rows.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- **I. Privacy-First, Offline-Always** — ✅ PASS. Ingestion-time, on-box, deterministic; no network, no telemetry.
- **II. Tiered Intelligence** — ✅ PASS (N/A). No LLM call added; this is a Tier-0/deterministic parse-time change.
- **III. Single Model Slot** — ✅ PASS (N/A). No model loaded.
- **IV. SQLite as Single Source of Truth** — ⚠️ DIVERGENCE (pre-existing, not introduced here). Live store is
  bundled PostgreSQL per ADR-0015; this feature adds only an **additive migration** to that store. Logged in
  Complexity Tracking; no new divergence created by this feature.
- **V. Pending Actions Queue** — ✅ PASS (N/A). No dangerous tool calls.
- **VI. Persona-as-Config (TOML)** — ✅ PASS (N/A). No agent/persona change.
- **VII. Hardware-Adaptive** — ✅ PASS. Falls back gracefully (sidecar-absent → current prose path).
- **VIII. Incremental Delivery** — ✅ PASS. P1 (whole-table unit → counting/list) is an independently
  shippable MVP; per-row units (P2) and summary integration layer on top.
- **IX. Pakistan Government Legal Domain** — ✅ PASS. Tables in Pakistani legal instruments (Schedules,
  distribution-of-business). Generalising to minutes/audit-paras stays within official-document scope.
- **X. Officer-First UX** — ✅ PASS. Directly fixes an officer-facing correctness defect (wrong department count).
- **XI. Citation Discipline** — ✅ PASS / strengthened. Whole-table and row units carry the table's place in
  the hierarchy (Schedule → table → row) so answers over tables remain citable.
- **XII. Latest Text + Amendment Chain** — ✅ PASS. Row-level amendment notes (inserted/deleted vide
  notification) are preserved with the row (FR-010), consistent with the amendment-chain principle;
  unrecognised markers tolerated.
- **XIII. Cross-Reference Graph as First-Class** — ✅ PASS / advances it. Adds the table as a first-class graph
  node with row children (FR-005), so structure walks and graph-guided summaries traverse it. NOTE: the
  constitution scopes graph population to the *deep tier*; this project already builds the graph on the fast
  tier (pre-existing divergence, same as ADR-0007/0019 work) — logged, not newly introduced.

**Gate result: PASS** (one pre-existing §IV divergence logged; no new violations).

## Project Structure

### Documentation (this feature)

```text
specs/023-structured-table-capture/
├── plan.md              # This file
├── research.md          # Phase 0 output — threading mechanism decision + alternatives
├── data-model.md        # Phase 1 output — table unit/node entities + migration
├── quickstart.md        # Phase 1 output — how to validate (re-index RoB → 39)
├── contracts/
│   └── table-unit-contract.md   # internal contract: whole-table + row unit shape, graph node shape
└── tasks.md             # /sp.tasks output (NOT created here)
```

### Source Code (repository root)

```text
src-tauri/src/services/
├── parsers/
│   ├── table_sidecar.rs     # (reuse) pdfplumber → SidecarTable{header, rows}
│   ├── table.rs             # (edit) reflow_with_sidecar: also surface structured tables + region markers
│   └── pdf.rs               # (edit) carry ExtractedTable list out of extraction alongside text
├── structure_parser.rs      # (edit) recognise a marked table region → a Table DocNode (level), not clause-parsed
├── structured_importer.rs   # (edit) persist whole-table + per-row chunks with unit_type; suppress prose in region
├── indexer.rs               # (edit, if needed) same for the non-structured/dispatch path
└── graph_builder.rs         # (edit) emit table node + row-child 'contains' edges
src-tauri/src/db/…/migrations/
└── NNN_table_units.sql      # (new, additive) widen unit_type + node_type CHECK vocab
```

**Structure Decision**: Single Rust workspace (`src-tauri`). Changes are localised to the ingestion parse→
chunk→graph path plus one additive migration; no new modules beyond the migration file and this feature's docs.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Constitution §IV (SQLite) — feature adds to bundled **PostgreSQL** instead | Pre-existing platform: the live store is PostgreSQL per ADR-0015; this feature must persist to the store that actually runs in prod | Writing to a separate SQLite file would fork the source of truth and break every existing read path — strictly worse. This feature introduces **no new** store; it only adds an additive migration to the existing one. |
