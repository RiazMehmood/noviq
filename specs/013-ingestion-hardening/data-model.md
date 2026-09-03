# Phase 1 — Data Model: Ingestion Hardening

This feature mostly **reuses existing tables**. No entity is invented; the model below maps the spec's
Key Entities onto the live schema (`src-tauri/src/db/pg/migrations/001_baseline.sql`,
`002_chunks_boilerplate.sql`, `003_chunks_provenance.sql`).

---

## Entity: Provision  → `chunks` (one row per structural unit)

| Spec attribute | Column | Notes / change in this feature |
|---|---|---|
| structural level | `unit_type` | `document\|part\|chapter\|article\|clause\|subclause\|section\|leaf`. **P2/D10**: catchlines typed `article` (not `clause`); add Chapter nodes. |
| numeric/alpha identifier | `unit_label`, `raw_marker` | **P2/D2/D3**: capture `7`,`10A`,`19A`,`25A`. |
| source text span(s) | `char_start`, `char_end`, `content` | **P2/D4**: catchline+body merged; spans preserved → verbatim invariant. |
| source page | `page_number` | **P0/D12**: currently NULL for provisions → set from `page_offsets`. |
| citation word (or unresolved) | `unit_path` / profile-applied label, `label_status` | **P3/D1**: word bound by resolver; conflict → `label_status='unresolved'`, no wrong word. |
| parent provision | `parent_chunk_id` | **P2/D3**: Article→Chapter→Part parentage correct. |
| confidence | `boundary_confidence` | low confidence → `label_status='unresolved'`. |
| block classification | `block_class` | `provision` default; `chrome` / `footnote` for non-body. |

**Invariants**
- Verbatim-substring: whitespace-normalized `content` ⊆ whitespace-normalized source (SC-006).
- `page_number ∈ [1, page_count]`, non-decreasing in reading order (SC-005, US3).
- No `provision` chunk contains footnote/amendment-history text or a footer page integer (SC-003).
- A provision is never labelled with the wrong citation word; uncertain → `unresolved` (SC-002).

## Entity: Footnote → `chunks` with `block_class='footnote'`

| Spec attribute | Storage |
|---|---|
| text | `content` (verbatim) |
| source page | `page_number` |
| anchor reference to marked provision (or unresolved) | `domain_metadata` JSON `{ "anchor_ref": "<provision id/label>" }` + `label_status` (`unresolved` when marker not confidently located). Add migration-004 column only if a query path requires it. |

- Kept separate from provisions, retrievable & auditable (US2 AC-1, FR-005).
- Anchored by **marker position**, not by same-page heuristic (research D-3).

## Entity: Document profile → `doc_profiles::*` (code, not a table)

`constitution` / `act` / `rules` / `manual` / `regulation` / `generic`. Feeds layer 1 of the
vocabulary resolver (research D-5). **P3/D1**: `generic.rs` stops emitting `Section {num}` at
detection time; the word is bound by the resolver.

## Entity: Amendment relationship → `amendment_events` (+ `graph_edges` `amended_by`)

| Spec attribute | Column |
|---|---|
| source provision | `amendment_events.node_id` → `graph_nodes.id` |
| amending instrument (verbatim) | `by_instrument` |
| amendment kind | `kind` ∈ `inserted\|substituted\|repealed\|omitted` |
| effective date (where stated) | `effective_date` |
| resolved/unresolved | omit/flag the event when the anchor provision is uncertain (FR-014) |

Edge form: `graph_edges(from_node=provision, to_node=instrument-node, edge_type='amended_by')`.
Derived from **isolated footnotes** (so depends on P1). Identifiers verbatim or flagged (SC-009).

## Entity: Cross-reference relationship → `graph_edges` `cross_reference`

| Spec attribute | Storage |
|---|---|
| source provision | `from_node` → `graph_nodes.id` |
| target identifier | `to_node` (resolved) or flagged unresolved when target not found |
| resolved/unresolved | unresolvable target flagged, never invented (FR-015, SC-009) |

`graph_builder::article_refs()` already extracts in-text "Article N" references; **US5** turns those
into `cross_reference` edges. Captured verbatim from source text.

---

## State transitions (label resolution)

```
detected (identifier + NodeLevel, no word)
        │  vocabulary resolver (profile → linguistic evidence → override)
        ▼
   ┌─────────────┐         conflict / insufficient evidence
   │  resolved   │ ◀──────────────┐
   │ citation_unit set            │
   └─────────────┘                │
        ▲                         ▼
        │                  ┌──────────────┐
        └── confident ─────│  unresolved  │ citation_unit = NULL, label_status='unresolved'
                           └──────────────┘
```

No transition ever synthesizes an identifier or a citation word; the only "uncertain" sink is
`unresolved` (flag-don't-fabricate).
