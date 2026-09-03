# Implementation Plan: Re-chunk & Re-index for Citation-Grade Retrieval (010 extension)

**Extends**: Feature `010-one-ask-drafting` | **Branch**: `011-postgres-pgvector-port` (no new branch) | **Date**: 2026-06-15
**Spec**: [spec-rechunk-reindex-extension.md](spec-rechunk-reindex-extension.md)

## Summary

The corpus is shredded (median chunk 41c, 55% under 50c); governing provisions exist only as scraps, so citation-grade drafting cannot find them. Fix the **chunking** so a provision becomes one coherent, correctly-labelled, clean-text unit, then **re-index from source** through the existing path so the improvement reaches the live corpus. Three code changes feed one existing re-index pipeline — **no new chunking path, no new re-index mechanism**.

## Technical Context

**Language/Version**: Rust 1.87 (`src-tauri`), TypeScript/SolidJS (minor settings UI only)
**Primary Dependencies**: existing only — `structure_parser` (chunk tree), `text_normalizer` (OCR repair), `intent_router::decompose` (label helpers), `fastembed` (re-embed), bundled Postgres + pgvector (011), `indexer::do_index` (re-index path)
**Storage**: bundled Postgres `echo` DB (per 011) — `chunks` (+ generated `tsv`), `chunk_embeddings`, `graph_nodes`
**Testing**: `cargo test` — unit tests on the coalescing transform + repair + label functions (pure, no DB); golden-corpus assertions on a Constitution fixture (no-regress) and a fragmented-rule fixture
**Target Platform**: Linux desktop (Tauri), offline
**Project Type**: single (Rust workspace + Tauri frontend)
**Performance Goals**: re-index of the ~10-doc Sindh corpus completes in the same envelope as today's fresh index (the T018 gate: ~20k chunks, minutes, 0 oomd); coalescing reduces chunk count so embedding work decreases, not increases
**Constraints**: fully offline; verbatim citation contract preserved; no regression on already-coherent docs; single model slot untouched (this feature adds no LLM calls)
**Scale/Scope**: ~10 documents / ~20k chunks today → fewer, larger coherent units after coalescing

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I. Privacy-First, Offline-Always | ✅ PASS | No network; all transforms + re-embed are local. |
| II. Tiered Intelligence | ✅ N/A | No new LLM calls; coalescing is deterministic text processing (below Tier 1). |
| III. Single Model Slot | ✅ PASS | Re-embed uses existing fastembed (not an LLM slot); no second model loaded. |
| IV. Single Source of Truth | ⚠️ INHERITS 011 | Persistence is the bundled Postgres from 011 (whose §IV amendment ADR is the pending merge gate). This feature adds **no** new DB/files and writes only existing tables — it introduces no *new* §IV exposure. |
| V. Pending Actions Queue | ✅ PASS | A full re-index is a heavy, destructive-to-old-index operation → classify the trigger as **CAUTIOUS/DANGEROUS** with explicit confirmation (FR-008/009), consistent with §V. |
| VII. Hardware-Adaptive | ✅ PASS | Re-index already respects tier; coalescing lowers chunk volume. |
| VIII. Incremental Delivery | ✅ PASS | US1+US4 (coalesce + safe re-index) ship as the MVP; US2 (repair) and US3 (labels) layer on independently. |

**Gate result**: PASS (the only flag, §IV, is inherited from 011 and not worsened).

## Key Decision (Phase 0 research)

**Decision: implement a GENERIC small-leaf coalescing pass in `structure_parser` (mirroring `split_oversized_leaves`). Do NOT revive Feature 007's `chunks_replace`.**

- **Rationale**:
  - 007's `deep_reindex/chunks_replace.rs` is a **2-line stub** — there is no design to "revive," only an intent in 007's tasks. Worse, it is bound to the **unbuilt deep-IR/Mistral tier** (007 US2, never implemented), which would drag in a 7B pipeline this feature explicitly doesn't need.
  - `structure_parser::split_oversized_leaves` already establishes the exact pattern: a post-order tree transform applied to every parsed doc on the **fast tier** (which is what the whole corpus uses). The cure is its **inverse** — `coalesce_undersized_leaves` — merging adjacent under-threshold sibling leaves under the same parent, preserving the parent citation/label. This keeps **one** chunking path (split for too-big, coalesce for too-small), applies to all docs, and is unit-testable without any model.
- **Alternatives considered**:
  - *Revive 007 chunks_replace (deep tier)* — rejected: unbuilt, model-heavy, narrower (deep tier only), violates "no third path."
  - *Post-hoc DB merge of small chunk rows* — rejected: loses the hierarchy/label context the tree carries; would reinvent labelling.
  - *Bump the existing oversize threshold only* — rejected: doesn't address under-sized fragments at all (the actual problem).

**Three components, one pipeline**:
1. **Coalescing (US1, FR-001/002/003/010)** — new `coalesce_undersized_leaves(node, min, max)` in `structure_parser.rs`, applied right after `split_oversized_leaves` in the same tree-finalize step. Merges consecutive sibling leaves under a common parent until a target (~300–800c) is reached; never merges across a non-leaf/structural boundary; never merges distinct named provisions; leaves a genuinely short standalone provision intact. Coherent unit inherits the parent's citation + a `/m1` style `unit_path` so labels resolve.
2. **OCR/text repair (US2, FR-004)** — extend existing `text_normalizer` with a **conservative intra-word-space / broken-line repair** ("o ne hundred"→"one hundred", "sha ll"→"shall") that runs before structure detection. Must be verbatim-safe: only join splits that reform a dictionary word / known legal token; reviewed heuristics, idempotent, no change to already-clean text.
3. **Label fix (US3, FR-005/006)** — strengthen `card_label`/structure label capture so units carry the hierarchical reference ("Rule 42(1)(ii)") and a year in a heading is never taken as the provision number. Reuses `decompose` label helpers already extended hyphen-tolerant.

**Apply via the existing re-index path (US4, FR-007/008/009/012)** — drive through `indexer::do_index` (the fresh-re-index-from-source path 011 already uses), reusing its FK-safe chunk wipe, progress events, and resumability. No new migration, no new command surface beyond a Settings "Re-index corpus" trigger (CAUTIOUS/DANGEROUS per §V) that rebuilds with the old corpus kept queryable until completion.

## Project Structure

### Documentation (this extension)
- `specs/010-one-ask-drafting/spec-rechunk-reindex-extension.md` (spec)
- `specs/010-one-ask-drafting/plan-rechunk-reindex-extension.md` (this file)
- `specs/010-one-ask-drafting/checklists/rechunk-requirements.md`

### Source code (existing files touched — no new modules except tests)
```
src-tauri/src/services/structure_parser.rs   # + coalesce_undersized_leaves (mirror of split_oversized_leaves)
src-tauri/src/services/text_normalizer.rs     # + conservative intra-word/broken-line repair (verbatim-safe)
src-tauri/src/services/intent_router/decompose.rs  # label helpers (reuse; minor hardening for year-not-number)
src-tauri/src/services/indexer.rs             # re-index path reuse (no new mechanism)
src-tauri/src/commands/<indexing>.rs          # Settings "Re-index corpus" trigger (CAUTIOUS/DANGEROUS)
src-tauri/tests/                              # coalescing + repair + label units; Constitution no-regress + fragmented-rule golden
```

**Structure Decision**: Single project; extend existing services. The whole feature is additive to the chunk-tree finalize step + a Settings trigger; it deliberately reuses the indexer, embedder, and DB layers untouched.

## Phasing (delivery order; per §VIII each slice is independently testable)

- **Slice 1 (P1 — MVP): coalescing + safe re-index.** `coalesce_undersized_leaves` + wire into tree finalize + Settings re-index trigger. Gate: re-index the corpus; the Direct-Contracting rule retrieves as one coherent unit (SC-002); median length up, <50c share down (SC-001); Constitution not regressed (SC-005).
- **Slice 2 (P2): OCR/text repair.** Extend `text_normalizer`; gate on clean-quote sample (SC-003) + verbatim gate still holds (FR-011).
- **Slice 3 (P3): label fix.** Hierarchical labels + year-not-number; gate on label sample (SC-004).

(Re-embedding happens for free on each slice's re-index — fastembed runs in the existing path.)

## Risks & Mitigations (≤3)

1. **Over-merging distinct provisions** (regresses the Constitution). → Coalesce only consecutive leaves under the *same* parent, stop at named-provision boundaries, hard max size; golden Constitution no-regress test as a gate (SC-005).
2. **Text-repair altering wording** (breaks verbatim contract). → Repair only reforms splits into known words/tokens, idempotent, reviewed; the citation/verify gate (FR-011) re-confirms every quote exists in source after repair — anything unverifiable is flagged, never invented.
3. **Re-index cost / interruption** on the user's NTFS+bundled-PG setup. → Reuse 011's proven re-index path (T018 gate: minutes, 0 oomd) and its FK-safe wipe + resumability; keep old corpus queryable until completion (FR-008/009).

## Open items for /sp.tasks

- Confirm exact coalescing thresholds (min-merge / target / hard-max) — tune against SC-001/SC-005 on a sample re-index.
- Decide whether the re-index trigger is whole-corpus only or also per-document (default: whole-corpus, simplest).
