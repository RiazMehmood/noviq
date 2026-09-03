# Tasks: General Structured-Table Capture at Ingestion

**Feature**: `023-structured-table-capture` | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Input**: spec.md (US1/US2/US3), plan.md, research.md (R1 marker+sidecar), data-model.md, contracts/table-unit-contract.md (C-1..C-6)

## Conventions

- **[P]** = parallelizable (different file, no incomplete dependency)
- **[US1/US2/US3]** = user-story phase task; setup/foundational/polish carry no story label
- Build/test on the VM: `cargo test --lib --no-default-features --features llm,server,cuda <filter>` (no local Rust toolchain)
- All paths under `src-tauri/src/` unless noted

---

## Phase 1: Setup

- [ ] T001 Resolve exact CHECK constraint names for `chunks.unit_type` and `graph_nodes.node_type` on the live `echo` db (`\d chunks`, `\d graph_nodes`; PG port drifts — `ss -tlnp|grep postgres`) and record them in `specs/023-structured-table-capture/data-model.md`.

---

## Phase 2: Foundational (blocking prerequisites for all stories)

**Purpose**: schema + the `ExtractedTable` type + marker plumbing that every story builds on.

- [ ] T002 Add additive migration `src-tauri/src/db/pg/migrations/013_table_units.sql` widening `chunks.unit_type` CHECK to include `'table','table_row'` and `graph_nodes.node_type` CHECK to include `'table','table_row'` (superset; DROP+ADD constraint, no data change) per data-model.md.
- [ ] T003 [P] Define `ExtractedTable { id: usize, page: u32, header: Vec<String>, rows: Vec<Vec<String>> }` in `services/parsers/table.rs` (pub), plus a stable region-marker helper (`⟦TBL:{id}⟧` / `⟦/TBL:{id}⟧` open/close constants + a `strip_table_markers(&str) -> String`).
- [ ] T004 Edit `services/parsers/table.rs::reflow_with_sidecar` to (a) wrap each linearised table's rows with the open/close markers carrying its `id`, and (b) also build and return the `Vec<ExtractedTable>` (id, page, header, normalised rows) alongside the existing `(text, emitted_rows)` — new return field/struct. Keep row normalisation (enumerator-row drop, single-cell collapse) as today.
- [ ] T005 Thread the `Vec<ExtractedTable>` out of extraction: extend the `pdf.rs` extraction result (and `maybe_reflow_tables`) to carry `tables: Vec<ExtractedTable>` (default empty when sidecar absent/no tables — FR-008 fallback), up to the parse-result type consumed by `structure_parser`/`structured_importer`.
- [ ] T006 [P] Update golden tests in `services/parsers/table.rs` for the marker-wrapped reflow output, and add a marker round-trip unit test (`strip_table_markers(reflow(...))` == pre-marker text) — protects C-4 and the golden-gate flagged in research.md.

**Checkpoint**: schema widened; `ExtractedTable` + markers flow from sidecar to the structuring stage; reflow tests green.

---

## Phase 3: User Story 1 — whole-table unit → count/list complete (Priority: P1) 🎯 MVP

**Goal**: one whole-table unit per detected table holding all rows, bypassing the clause parser, so counting/list return the full set (RoB → 39).

**Independent test**: re-index RoB; `SELECT count(*) FROM chunks WHERE source_id=<RoB> AND unit_type='table'` ≥ 1 and its body-row count = 39; assistant "how many departments" → 39, "list all" → all 39.

- [ ] T007 [US1] In `services/structure_parser.rs`, recognise a `⟦TBL:{id}⟧…⟦/TBL:{id}⟧` region as an **atomic** table node (new Table level under the enclosing structural node) instead of feeding it to the clause/heading detector — the region is not sub-clause-parsed (FR-004).
- [ ] T008 [US1] In `services/structured_importer.rs`, when persisting a table region, emit the **whole-table chunk** from the matching `ExtractedTable`: `unit_type='table'`, markdown (header + separator + all rows), `unit_label`/`unit_path` per contract; strip markers from stored content; size-segment only on whole rows if over budget (data-model validation). Suppress any prose chunk that would fall inside the region (SC-003).
- [ ] T009 [US1] In `services/graph_builder.rs`, emit a `node_type='table'` graph node for the whole-table chunk, `contains`-linked from its enclosing structural node (FR-005).
- [ ] T010 [US1] Wire the counting path (Track 1 / `services/counting.rs`) to recognise `node_type='table'`/`'table_row'` so "how many <rows-subject>" counts the table's rows (deterministic SQL) — returns 39 for RoB Schedule-I.
- [ ] T011 [P] [US1] Unit tests in `structured_importer` (or a new `table_units` test module): C-1 (whole-table body-row count == N), C-2 partial (1 whole-table chunk per table), C-6 (no table unit when `rows` empty), and a synthetic 3-col/39-row `ExtractedTable` → one markdown chunk with 39 body rows.

**Checkpoint**: US1 independently shippable — whole-table unit + counting deliver RoB=39. MVP complete.

---

## Phase 4: User Story 2 — summary covers table content (Priority: P1)

**Goal**: the whole-document summary represents the table (subject + scale) rather than omitting it.

**Independent test**: regenerate RoB `document_full`; summary mentions the department table and its scale (~39) — SC-005.

- [ ] T012 [US2] Confirm/adjust `services/summary_builder.rs::load_content_for_summary` includes the `unit_type='table'` chunk (not filtered as chrome/footnote) so `build_document_full` sees the whole table in reading order (FR: table is ordinary content for the current summarizer).
- [ ] T013 [P] [US2] Unit test: `load_content_for_summary` returns the table chunk for a source containing one; and the table chunk sorts in document reading order (by `chunk_index`) relative to surrounding prose.

**Checkpoint**: US2 independently verifiable — a table-bearing doc's summary no longer drops the table. (Deep graph-walking summarization is feature 024, out of scope here.)

---

## Phase 5: User Story 3 — targeted row retrieval (Priority: P2)

**Goal**: per-row units so a single row is retrievable with header context.

**Independent test**: `SELECT count(*) FROM chunks WHERE source_id=<RoB> AND unit_type='table_row'` = 39; ask "what falls under the Works Department" → that row's cells.

- [ ] T014 [US3] In `services/structured_importer.rs`, additionally emit one **`table_row` chunk per row** (`content = "col: cell | col: cell …"` with header labels; amendment note retained inline — FR-010); `unit_label`/`unit_path` per contract.
- [ ] T015 [US3] In `services/graph_builder.rs`, emit a `node_type='table_row'` node per row chunk, `contains`-linked from the table node (FR-005; enables C-3 and the graph walk).
- [ ] T016 [P] [US3] Unit tests: C-2 full (exactly `1 + N` chunks per table), C-3 (table node has N `table_row` children), and per-row header-context + amendment-note retention.

**Checkpoint**: US3 independent — targeted row lookup works; graph table→rows walk complete.

---

## Phase 6: Polish & Cross-Cutting

- [ ] T017 [P] Regression guard test: a prose-only fixture (no tables) yields **zero** `table`/`table_row` units and byte-identical prose chunks vs pre-feature (SC-004 / C-5 / C-6).
- [ ] T018 [P] False-positive guard test: an enumerated-clause fixture that *looks* columnar (the `table_row` over-tag trap) produces **no** `table` unit (units come only from `ExtractedTable`) — FR-009.
- [ ] T019 Full VM lib test run (`cargo test --lib --no-default-features --features llm,server,cuda`) — confirm 0 regressions across existing parser/graph/summary tests.
- [ ] T020 Live acceptance per quickstart.md: deploy branch → `POST /api/reindex_structure` → verify RoB `table`=1(+segments)/`table_row`=39, counting=39, list=39, summary covers the table; spot-check one other table-bearing doc; confirm a prose-only doc unchanged.
- [ ] T021 [P] Update `CHANGELOG_DEV.md` (new entry: general structured-table capture, marker+ExtractedTable design, migration 013, RoB=39 result) and surface an ADR suggestion if the graph table-node shape proves architecturally significant.

---

## Dependencies & Execution Order

- **Setup (T001)** → **Foundational (T002–T006)** block everything.
- **US1 (T007–T011)** depends on Foundational; it is the **MVP** and must land first.
- **US2 (T012–T013)** depends on US1 (needs the `table` unit to exist).
- **US3 (T014–T016)** depends on Foundational + US1's persist path (shares `structured_importer` table-emit + `graph_builder`); can follow US1.
- **Polish (T017–T021)** after the stories it verifies.

**Story independence**: US1 alone delivers the headline fix (count/list = 39). US2 and US3 are additive increments on top.

## Parallel opportunities

- Foundational: T003 and T006 are `[P]` (distinct concerns) once T004's shape is fixed.
- Within US1: T011 `[P]` (tests) alongside T009/T010.
- US3 tests T016 `[P]`; Polish T017/T018/T021 `[P]` (independent files).

## Implementation strategy (MVP-first)

1. Land **Setup + Foundational + US1** → ship the MVP (RoB counting/list = 39). Verify live.
2. Add **US2** (summary coverage) and **US3** (row retrieval) as independent increments.
3. **Polish** regression/false-positive guards + live acceptance + changelog.

## Task count

- Total: **21** (Setup 1, Foundational 5, US1 5, US2 2, US3 3, Polish 5).
- Suggested MVP: **T001–T011** (Setup + Foundational + US1).
