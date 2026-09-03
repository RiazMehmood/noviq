# Phase 0 Research: General Structured-Table Capture

## R1 — How does the authoritative table structure reach unit creation?

**Context**: `table_sidecar::extract_tables` / `extract_document` produce `SidecarTable{header, rows}` inside
the parser layer (`pdf.rs` → `table::reflow_with_sidecar`). Today only the *linearised text* and a flat list
of emitted row lines survive; unit creation (`structure_parser`/`structured_importer`) sees text only, and
the clause parser shreds the table. We must carry the structure to unit creation without a large re-plumb.

**Decision**: **In-band region markers + a structured `ExtractedTable` sidecar carried on the parse result.**
- `reflow_with_sidecar` wraps each linearised table's rows with lightweight sentinel lines that encode a
  stable table id (e.g. `⟦TBL:{n}⟧ … rows … ⟦/TBL:{n}⟧`), and returns, alongside the text, a
  `Vec<ExtractedTable>` where `ExtractedTable = { id, page, header: Vec<String>, rows: Vec<Vec<String>> }`.
- The parse result (`pdf.rs`) carries `tables: Vec<ExtractedTable>` next to the text (new field; empty when
  no sidecar / no tables).
- At unit creation, a region between `⟦TBL:n⟧…⟦/TBL:n⟧` is treated as **atomic**: it is NOT clause-parsed;
  instead the matching `ExtractedTable[n]` is used to emit a whole-table unit + per-row units, and the
  sentinels are stripped from stored content.

**Rationale**:
- Reuses the existing text flow (markers ride through every layer that already passes text) → **smallest
  viable diff**, no re-architecture of the parser→chunker interface.
- The structured `ExtractedTable` (not the linearised text) is the source for unit content, so cell/column
  fidelity is preserved (markdown table for the whole-table unit; header+cells per row).
- Deterministic; the marker is an internal sentinel never shown to users (stripped before storage).

**Alternatives considered**:
- *Typed threading only (no markers)* — thread `Vec<ExtractedTable>` through every stage and re-locate each
  table's text region by matching. Rejected: re-locating the region in already-reflowed text is fragile
  (whitespace/normalisation drift) and touches more signatures than the marker approach.
- *Rely on existing `block_class='table_row'` tags* — rejected in the spec: the tag is **noisy**
  (over-tags ordinary enumerated clauses; scattered across the whole doc), so grouping by it captures wrong
  content.
- *Post-hoc region deletion* (chunk normally, then delete shredded table chunks and inject clean ones) —
  rejected: identifying which of the 292 shredded chunks belong to the table is exactly the fragile
  region-match problem, after the damage is done.

**Follow-up**: markers change reflow output, which is golden-gated in `table.rs` tests. Gate marker emission
so golden text tests either (a) assert the stripped form, or (b) update goldens to include sentinels. Plan
task will pick (update goldens) and add a marker round-trip test.

## R2 — What is the whole-table unit's content shape?

**Decision**: A compact **GitHub-flavoured markdown table** (header row + separator + one line per row),
capped to the existing per-chunk size budget; if a single table exceeds the budget, split into ordered
whole-table *segments* that each retain complete rows (never split mid-row) so every row is still present
and countable across the segments.

**Rationale**: markdown tables are (a) token-efficient, (b) trivially countable (row lines), (c) render
cleanly if surfaced, and (d) already how the codebase linearises tabular content. Counting = number of body
row lines; enumeration = read the rows.

**Alternatives**: JSON blob (heavier, not answer-friendly); raw linearised prose (loses columns → the current
failure). Rejected.

## R3 — Unit/node typing and the graph shape

**Decision**: Introduce two chunk `unit_type` values — **`table`** (whole-table unit) and **`table_row`**
(per-row unit) — and two graph `node_type` values — **`table`** and **`table_row`** — with the table node
`contains` its row nodes (reusing the existing `contains` edge). The table unit attaches under its enclosing
structural node (e.g. the `Schedule` container) so its hierarchical path is preserved for citations.

**Rationale**: distinct types let retrieval, counting, and the graph-guided summarizer treat tables as
first-class (Constitution XIII) without special-casing prose; `contains` reuse means the existing
recursive-CTE walk (`graph::expand`) already traverses table→rows for free.

**Note on `block_class` vs `unit_type`**: `block_class='table_row'` (noisy layout tag) is orthogonal and left
as-is; this feature keys off `unit_type`/`node_type`, which are authoritative and set only for real detected
tables.

## R4 — Idempotent re-index & migration safety

**Decision**: Table units/nodes are written in the same delete-then-insert path `do_index` already uses
(`graph::delete_for_source` + `chunk_repo::delete_by_source` before re-persist), so re-index replaces them
cleanly (FR-011). The migration only **widens two CHECK constraints** (additive vocab extension); on
PostgreSQL this is `ALTER TABLE … DROP CONSTRAINT … ADD CONSTRAINT …` with the superset — no data change, no
column/table drop (Development Constraint: additive migrations).

**Rationale**: reuses proven idempotency; CHECK-widening is the minimal schema change and is reversible.

## R5 — Counting & summary consumption (interfaces this feature must satisfy)

**Decision**: This feature's output must satisfy two existing consumers without changing them here:
- **Counting (Track 1, deterministic SQL over graph nodes)**: "how many departments" resolves to counting
  `table_row` child nodes of the relevant `table` node → 39. (Wiring the counting query to recognise the new
  node types is a small follow-on task in this feature's scope.)
- **Whole-document summary (`build_document_full`)**: the whole-table unit is ordinary content in
  `load_content_for_summary`, so it is included today; the *graph-guided* summarizer (separate feature 024)
  will additionally walk the table node to enumerate rows.

**Rationale**: keeps this feature's blast radius at "produce correct units/nodes"; consumers are already
shaped to read them.

## Resolved unknowns

All Technical Context items are known (existing stack, no new deps). No remaining NEEDS CLARIFICATION.
