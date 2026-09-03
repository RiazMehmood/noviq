# Phase 1 Data Model: General Structured-Table Capture

## New in-memory types (parser layer)

### `ExtractedTable`
The authoritative detected-table structure carried from reflow to unit creation.

| Field | Type | Notes |
|---|---|---|
| `id` | `usize` | Stable per-document index; matches the `⟦TBL:{id}⟧` region marker in text |
| `page` | `u32` | 1-based source page where the table starts |
| `header` | `Vec<String>` | Column labels; may be empty (best-effort — FR: ragged tables) |
| `rows` | `Vec<Vec<String>>` | Ordered body rows, each a vector of cell strings (enumerator/`(1)(2)` rows already dropped by existing reflow normalisation) |

- Carried on the parse result (new field `tables: Vec<ExtractedTable>`, default empty).
- Empty when the sidecar is unavailable or the document has no detected tables (FR-008 fallback).

### `TableNode` (a `DocNode` variant/level)
At tree build, a marked table region becomes a structural node instead of clause-parsed prose.

- Level: a new **Table** level (below the enclosing structural container, e.g. `Schedule`/`Part`).
- Children: one child per row.
- Citation: `"<enclosing citation> — Table"` (and per row `"… — Table row N"`), preserving hierarchy (XI).

## Persisted entities (existing tables, new type values)

### `chunks` — two new `unit_type` values
| unit_type | Content | Purpose |
|---|---|---|
| `table` | Whole table as compact markdown (header + separator + all body rows); may be split into ordered segments only if size-capped, never mid-row | Counting / list-all / summary — the complete set in one place |
| `table_row` | One row rendered with header context (`"Header: … \| cells …"`) | Targeted retrieval ("what falls under Department X") |

- `unit_path` carries the table's place in the hierarchy (e.g. `Schedule-I/Table/row-7`).
- Row amendment notes (inserted/deleted vide notification) stay inside the row's cell text (FR-010).
- Prose chunks that would have been produced *inside* a table region are **suppressed** (the region is
  atomic) — eliminating the 292-fragment shredding (SC-003).

### `graph_nodes` — two new `node_type` values
| node_type | chunk_id → | Edges |
|---|---|---|
| `table` | the whole-table chunk | `contains` → each `table_row` node (reuses existing tree edge) |
| `table_row` | the per-row chunk | (leaf) |

- The `table` node is `contains`-linked *from* its enclosing structural node, so `graph::expand` walks
  document → … → Schedule → table → rows with no new traversal code (Constitution XIII).

## Migration `013_table_units.sql` (additive)

Widen two CHECK constraints to include the new vocab; no column/table drop, no data change.

```sql
-- chunks.unit_type: add 'table', 'table_row'
ALTER TABLE chunks DROP CONSTRAINT <chunks_unit_type_check>;
ALTER TABLE chunks ADD  CONSTRAINT <chunks_unit_type_check>
  CHECK (unit_type IN ('document','part','chapter','article','clause','subclause','section','leaf','table','table_row'));

-- graph_nodes.node_type: add 'table', 'table_row'
ALTER TABLE graph_nodes DROP CONSTRAINT <graph_nodes_node_type_check>;
ALTER TABLE graph_nodes ADD  CONSTRAINT <graph_nodes_node_type_check>
  CHECK (node_type IN ('document','part','chapter','article','clause','subclause','table','table_row'));
```

(Exact constraint names resolved from `\d chunks` / `\d graph_nodes` at implementation; the widened set is a
strict superset so existing rows always satisfy it.)

## Validation rules

- A `table` unit's body-row count MUST equal `ExtractedTable.rows.len()` (SC-002).
- Per detected table, total units emitted MUST be `1 + rows.len()` (one whole-table + one per row) (SC-003).
- If `ExtractedTable.rows` is empty → emit **no** table unit/node (edge case: table with no usable rows).
- No `table`/`table_row` unit may be produced from a region that was not a detected table (FR-009 — no
  false positives; guaranteed because units come only from `ExtractedTable`, never from the noisy layout tag).

## State / lifecycle

- Written in `do_index`'s existing delete-then-insert transaction → re-index replaces cleanly (FR-011).
- No state machine; units are immutable once written for a given source version.
