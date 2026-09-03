# Contract — Resolved Labels + Provenance (US2)

**Modules:** `structure_parser.rs` (resolve label from hierarchy at tree-build),
`structured_importer.rs` (persist provenance; kill synthetic-id fallback),
migration `003_chunks_provenance.sql`.

## Core
```rust
/// At tree-build, set unit_label = resolved hierarchical citation (e.g. "Rule 27(i)")
/// and capture the raw marker + provenance. Replaces any silent synthetic id.
struct ResolvedChunk {
    unit_label: String,       // resolved citation OR unresolved-flag sentinel
    raw_marker: Option<String>,
    unit_path: String,        // existing
    block_class: BlockClass,  // provision | chrome | table_row
    page: Option<u32>,
    char_span: Option<(usize, usize)>,
    extraction_method: String,
    reconstruction_method: String,
    boundary_confidence: Option<f32>,
    label_status: LabelStatus, // Resolved | Unresolved
}
```

## Invariants
- **No silent synthetic id.** A parse that cannot assign a marker sets `label_status=Unresolved` and surfaces a flag — never emits `m2`/`m66` as `unit_label`. (Pin the current synthesis site during implementation.)
- `block_class='provision'` ⇒ `unit_label` is the resolved hierarchical citation (not a bare `(i)`).
- Migration `003` is **additive** (no drops); defaults keep existing rows valid (`block_class='provision'`, `label_status='resolved'`).
- G1: provenance `char_span` must bound the verbatim source of every number/quote in `content`.

## Acceptance
- No chunk cites a bare `(i)` or `m66` (US2 AC).
- Ambiguous hierarchy ⇒ `label_status='unresolved'` flag, never a confident-but-wrong resolved label.
- Provenance populated for new chunks; `cargo test --lib structured_importer` + `structure_parser` green.
