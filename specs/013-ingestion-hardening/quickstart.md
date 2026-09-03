# Quickstart — Ingestion Hardening (developer loop)

Deterministic, offline, no model. The loop is **one phase at a time**, re-index the 3-doc control
set, diff against the frozen fixture.

## Prerequisites

- Rust 1.87 workspace builds (`src-tauri`).
- `pdftotext` (poppler) on PATH — provides `-bbox` geometry and `-layout` text.
- Bundled PostgreSQL+pgvector running (app's normal dev DB at `{app_data_dir}/pg/data`).
- The 3 control docs in the test folder (`~/Desktop/ingestion_test_folder/`):
  simple prose, small table, `fundamental_rights.pdf` (digital law, the frozen 177-chunk fixture).

## Per-phase loop (LOCKED order: P0 → P1 → P2 → P3 → P4 → US5)

```bash
# 1. implement the single phase's targeted fix (see plan.md table)

# 2. unit gate
cargo test --lib            # must stay green; add the phase's new unit tests

# 3. re-index the control folder via the FAST path (no 7B).
#    Incremental indexing dedup-skips unchanged files, so a CODE-only change
#    won't reprocess them — force it by flipping the affected sources to pending:
#      UPDATE sources SET indexing_status='pending' WHERE id IN (...);
#    then trigger re-index (Index Folder / reindex button). Log must show
#    "fast-tier — skipping LLM IR".

# 4. dump chunks + diff against the frozen fixture
#    (chunk_captures TSV / ALL_chunks_by_document.xlsx under the test folder)

# 5. acceptance gate
cargo test --test ingestion_hardening_acceptance

# 6. log it: MODULAR_IMPROVEMENT_LOG.md (NOT CHANGELOG_DEV.md for this workstream)
#    + new dated rows in ~/Desktop/Noviq_Ingestion_Tracker.xlsx
```

Only when the phase's target defects are gone **and** no prior phase regressed → start the next phase.

## Phase acceptance checks (what "done" means)

- **P0 (D9, D12)**: no chunk contains `shallnot`; every provision has a valid, in-range,
  non-decreasing `page_number`; verbatim-substring property holds.
- **P1 (D5, D6)**: no provision chunk contains amendment-history text or a footer page integer;
  footnote chunks exist (`block_class='footnote'`), each with a page_number, anchored or `unresolved`.
- **P2 (D2, D3, D4, D10)**: Articles 7–40 (incl. 10A/19A/25A) all present as Article nodes; Art 7 ∈
  Part II; Art 8–28 ∈ Chapter 1; Art 29–40 ∈ Chapter 2; no orphan bare-label chunks; catchline not a
  separate chunk; 25/25A flagged (not silently reordered).
- **P3 (D1)**: no chunk emits `Section N`; emits `Article N` or `unresolved`.
- **P4 (D7, D8)**: each sub-clause its own unit ((a) doesn't swallow (b)); bracketed amendment text
  verbatim.
- **US5**: amendment events + cross-reference edges captured in `amendment_events` / `graph_edges`
  with zero synthesized identifiers; uncertain anchors/targets flagged.

## Regression invariants (must always hold)

1. **Verbatim-substring** — de-whitespaced chunk content ⊆ de-whitespaced source.
2. **Flag-don't-fabricate** — uncertainty → `unresolved` / `NULL`, never a confident-wrong label.
3. **Deterministic fast tier** — no model loaded; pure regex/geometry/sequence.
4. **Doc 1 (prose) stays clean** — improving the law doc must not regress the simple doc (SC-008).
