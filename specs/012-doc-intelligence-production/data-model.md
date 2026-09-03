# Phase 1 Data Model — Feature 012

Persistent state lives in the bundled PostgreSQL schema (`chunks`, `sources`).
This feature adds **additive** columns (Constitution: migrations additive; no
drops) and one new in-memory classification produced upstream of tree-build.

## Entity: Block (in-memory, pre-tree) — US1

Produced by chrome-strip before tree construction. Not persisted directly; its
classification flows into the chunk's `block_class`.

| Field | Type | Notes |
|-------|------|-------|
| `text` | String | verbatim span from `-bbox`/`-layout` |
| `page` | u32 | source page (1-based) |
| `y_band` | normalized f32 | vertical position band (for repetition signal) |
| `class` | enum `BlockClass` | `Provision` \| `Chrome` \| `TableRow` |
| `chrome_reason` | Option<enum> | `Repetition` \| `Margin` \| `FormField` (audit) |

**Invariant.** A block classified `Chrome` is never attached under a provision
node. A block with a provision identity (marker/heading) is never classified
`Chrome` (keep-bias, mirrors the superseded `is_boilerplate` provision-keep rule).

## Entity: Chunk (persisted) — additive columns (US2)

Existing columns retained: `id`, `source_id`, `unit_label`, `unit_path`,
`content`, `parent_chunk_id`, `is_boilerplate` (repurposed — see below), …

| New column (migration `003`) | Type | Story | Notes |
|------------------------------|------|-------|-------|
| `block_class` | TEXT NOT NULL DEFAULT 'provision' | US1/US2 | `provision` \| `chrome` \| `table_row`. Chrome chunks excluded from retrieval. |
| `raw_marker` | TEXT NULL | US2 | literal marker as seen ("(b)", "27", "iii") before resolution |
| `page` | INT NULL | US2 | source page of the chunk start |
| `char_span_start` | INT NULL | US2 | char offset in the source text |
| `char_span_end` | INT NULL | US2 | char offset (end) |
| `extraction_method` | TEXT NULL | US2 | e.g. `pdftotext-bbox`, `pdftotext-layout`, `ocr` |
| `reconstruction_method` | TEXT NULL | US2 | e.g. `tree`, `table-projection`, `coalesced` |
| `boundary_confidence` | REAL NULL | US2/US3 | heading-detection confidence; low → `label_status='unresolved'` |
| `label_status` | TEXT NOT NULL DEFAULT 'resolved' | US2 | `resolved` \| `unresolved` (replaces silent synthetic id) |

**Repurpose `is_boilerplate`.** Retained as the cheap retrieval-side exclusion
(`AND c.is_boilerplate = FALSE`), now set from `block_class = 'chrome'` at the
upstream classifier — not from the removed downstream `is_boilerplate()` function.
Optionally collapse to `block_class` later; kept for migration-additivity now.

**State / validation rules.**
- `label_status='unresolved'` ⇒ `unit_label` carries a flag sentinel, never a
  synthetic `m<N>` id. (US2 AC: no chunk cites `m66`.)
- `block_class='provision'` ⇒ `unit_label` is the resolved hierarchical citation
  (US2). `unit_label`'s leading provision number must equal the leading provision
  number of `content` (US3 split action; else split or flag).
- Every numeric/quote/reference/cell in `content` is a verbatim substring of the
  source digital text for that `char_span` (G1; verify-gate enforced).

## Entity: TableRowUnit (persisted as chunk, `block_class='table_row'`) — US5

| Field | Maps to | Notes |
|-------|---------|-------|
| `content` | chunk.content | linearized row, headers folded in, **verbatim cells** |
| `unit_label` | chunk.unit_label | resolved authority/row label where derivable |
| `reconstruction_method` | `table-projection` | distinguishes from prose tree units |
| cross-page | — | continuation rows merged before persist; repeated headers dropped |

**Invariant (G1).** Every cell substring is lifted verbatim from the digital
text; reflow may reorder/regroup, never synthesize. Ragged region ⇒ fall back to
`-layout` prose (no table_row units emitted).

## Golden fixture set (G2) — release gate artifact

A frozen set under test fixtures (real-corpus cases), three kinds:
1. **Provision** → expected resolved `unit_label` + verbatim `content` head.
2. **Table** → expected ordered rows + verbatim cells (Delegation-of-Financial-Powers).
3. **Chrome** → expected exclusion (masthead `Rule 345(3)(b)`, "P a g e", "Contact Us").

Diffed on every re-index; a diff is a release blocker until reviewed.
