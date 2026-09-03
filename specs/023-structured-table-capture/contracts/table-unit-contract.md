# Internal Contract: Table Units & Graph Nodes

This feature has **no new HTTP API** (ingestion-time). The contract is the internal shape of what ingestion
produces, which downstream consumers (retrieval, counting, summarizer) rely on.

## Producer → store contract

For each `ExtractedTable` with `rows.len() = N ≥ 1`, ingestion MUST persist exactly:

### 1 whole-table chunk
```
unit_type  = 'table'
unit_label = "<enclosing label> — Table"        # e.g. "Schedule-I — Table"
unit_path  = "<enclosing path>/Table"
content    = markdown:
             | col1 | col2 | col3 |
             |------|------|------|
             | r1c1 | r1c2 | r1c3 |
             | …    | …    | …    |   (all N rows; split into ordered 'table' segments only if size-capped, never mid-row)
```

### N table-row chunks (i = 1..N)
```
unit_type  = 'table_row'
unit_label = "<enclosing label> — Table row {i}"
unit_path  = "<enclosing path>/Table/row-{i}"
content    = "col1: r{i}c1 | col2: r{i}c2 | col3: r{i}c3"   # header-labelled cells; amendment note retained inline
```

### Graph nodes
```
node_type='table'      chunk_id=<whole-table chunk>     ← contains-edge FROM enclosing structural node
node_type='table_row'  chunk_id=<row chunk>             ← contains-edge FROM the table node   (× N)
```

## Invariants (testable)

| ID | Invariant |
|---|---|
| C-1 | body-row count of the `table` chunk == `N` (== `ExtractedTable.rows.len()`) |
| C-2 | exactly `1 + N` chunks emitted per detected table (1 `table` + N `table_row`); 0 if `N==0` |
| C-3 | the `table` node has exactly `N` `contains` children of node_type `table_row` |
| C-4 | region markers `⟦TBL:*⟧` never appear in stored `content` |
| C-5 | prose-only documents produce byte-identical units to pre-feature behaviour (no regression) |
| C-6 | no `table`/`table_row` unit exists for any source with no detected tables |

## Consumer expectations (satisfied elsewhere, listed for traceability)

- **Counting**: "how many X" over a table == count of `table_row` children of the table node.
- **List-all**: enumerate the `table` chunk's rows (or the row nodes).
- **Targeted**: retrieve a single `table_row` chunk by semantic/keyword match.
- **Summary (graph-guided, feature 024)**: walk to the `table` node, enumerate/aggregate its rows.
