# Generic Structured Document Importer

**Date**: 2026-05-24
**Status**: Design — pending approval
**Feature flag**: `[experimental] structured_importer = true` (until acceptance gate stable)
**Builds on**: Feature 003 (constitution importer + structure_parser + graph_builder + heading_detector), Feature 004 (three-tier extractor + extraction_method + quality_score + startup re-extract hook)

## Problem

Today EchoAI has two indexing paths:

1. **Structured** — `index_structured_doc` consumes a pre-built JSON IR (`constitution_ir.json`) and produces a 6-level hierarchy (document → part → chapter → article → clause → subclause) plus `graph_nodes` and `graph_edges`. Only the Pakistan Constitution uses this path.
2. **Generic PDF** — size-based chunking with shallow `document` / `section` / `leaf` units; produces **zero** `graph_nodes` and **zero** `graph_edges`.

Every PDF that isn't the Constitution falls into path 2 and loses citation hierarchy, graph traversal, and clean unit-level retrieval. The Feature 003 design comment in `doc_profiles/mod.rs` already anticipates this gap: *"the abstraction lets the same `structure_parser` serve the Sindh government corpus (Acts / Rules / Notifications) later — same six-level shape, different level words."*

This design fills that gap.

## Goal

Every document indexed by EchoAI — Constitution, Act, Rules, Manual, or anything else — is decomposed into the appropriate hierarchy for its structure and contributes to `graph_nodes` / `graph_edges`. A `rules`-typed document cites as "Rule 9(1)(a)". A book cites as "Chapter 3 / Section 2.1". No document falls through to pure size-based chunking unless structure genuinely cannot be detected.

## Non-goals

- **Multi-language structure detection**: v1 covers English-language legal documents. Urdu / Sindhi structured docs are a v2 follow-up.
- **PDF table extraction**: tables in legal docs are inlined into the leaf text as-is. Structured table parsing is out of scope.
- **Re-OCR with a better engine**: OCR quality improvement is its own initiative (queued as Feature 004 v2). This design only adds a text-normalization pass on existing OCR output.
- **A new edge type beyond `contains` / `cross_reference` / `non_obstante` / `amended_by`**: schema unchanged.

## Architecture

Unified 7-step pipeline; every document flows through every step. "Profile picked" may be `generic` but the code path doesn't branch.

```
┌─────────────────────────┐
│ 1. Extract              │  three-tier (lopdf → pdftotext → tesseract) — UNCHANGED
└──────┬──────────────────┘  produces extraction_method + quality_score
       ▼
┌─────────────────────────┐
│ 2. Normalize            │  NEW — OCR cleanup: `{`/`}`→`(`/`)` adjacent to a/0-9,
└──────┬──────────────────┘  hyphenation joins, smart-quote unification, small
       │                     misread dictionary. Idempotent. No-op on clean text.
       ▼
┌─────────────────────────┐
│ 3. Detect profile       │  NEW — regex score per profile → highest wins if ≥
└──────┬──────────────────┘  min_confidence; else LLM tiebreaker; else `generic`.
       │                     Persists to sources.doc_type. User-set value never
       ▼                     overwritten by auto-detection.
┌─────────────────────────┐
│ 4. Detect headings      │  NEW — per-profile heading_patterns → DocNode tree.
└──────┬──────────────────┘  Uses existing heading_detector/regex_pass.rs.
       │                     LLM rescue (heading_detector/llm_pass.rs) only when
       ▼                     pattern coverage < 10% of document.
┌─────────────────────────┐
│ 5. structure_parser     │  EXISTING (Feature 003) — tree → 6-level node graph
└──────┬──────────────────┘  Profile-driven; vocabulary already pluggable.
       ▼
┌─────────────────────────┐
│ 6. Store chunks +       │  EXISTING — one transaction per source. If anything
│    graph_builder        │  after Step 4 fails, fall back to size-based chunks
└──────┬──────────────────┘  and mark structure_status='failed'. Source is never
       │                     left in a half-structured state.
       ▼
┌─────────────────────────┐
│ 7. Embed + NER          │  EXISTING — unchanged. Re-runs on re-cut chunks.
└─────────────────────────┘
```

**Two new services** (`services/text_normalizer.rs`, `services/structured_importer.rs`). Steps 1, 5, 6, 7 reuse existing services unchanged. Step 4 reuses `heading_detector/regex_pass.rs` (currently dormant — built in Feature 003 for exactly this).

## DocProfile schema

The existing `DocProfile` struct gains three fields. Backwards-compatible: the `constitution` profile sets the new fields to empty defaults and continues using its structured-IR path (the IR import bypasses Steps 3 and 4).

```rust
pub struct DocProfile {
    // EXISTING (Feature 003)
    pub doc_type:       &'static str,
    pub level_vocab:    [&'static str; 6],
    pub citation_unit:  &'static str,
    pub salient_units:  &'static [&'static str],

    // NEW
    pub detection_patterns: &'static [DetectionPattern],
    pub heading_patterns:   &'static [HeadingPattern],
    pub min_confidence:     f32,  // 0.0–1.0
}

pub struct DetectionPattern {
    pub regex:  &'static str,
    pub weight: f32,
    pub level:  NodeLevel,
}

pub struct HeadingPattern {
    pub regex:        &'static str,  // captures: number + optional title
    pub level:        NodeLevel,     // which of the 6 storage levels
    pub citation_fmt: &'static str,  // e.g. "Section {num}" | "Rule {num}({sub})"
}
```

Storage vocabulary is **fixed** at the migration-013 CHECK level (`document/part/chapter/article/clause/subclause`). Display vocabulary is per-profile. A `rules` profile's "Rule 9" stores as `unit_type='article'`, `unit_label='Rule 9'`, citation rendered as `"Rule 9"` via `profile.citation_unit`.

## The four profiles (v1)

| Profile      | level_vocab (L0…L5)                                              | citation_unit | Seed heading patterns                                                                                                                                  |
|--------------|------------------------------------------------------------------|---------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|
| `constitution` (shipped) | document/part/chapter/article/clause/subclause       | "Article"     | (consumes structured IR; bypasses Steps 3–4)                                                                                                            |
| `act`        | document/part/chapter/section/subsection/clause                  | "Section"     | `^Chapter\s+([IVX\d]+)`, `^Section\s+(\d+)\.\s*(.*)`, `^\((\d+)\)\s` (sub-section), `^\(([a-z])\)\s` (clause)                                            |
| `rules`      | document/part/chapter/rule/subrule/clause                        | "Rule"        | `^Part\s+([IVX\d]+)`, `^Rule\s+(\d+)\.?\s*(.*)` or `^(\d+)\.\s+([A-Z])`, `^\((\d+)\)` (sub-rule), `^\(([a-z])\)` (clause)                                |
| `manual`     | document/part/chapter/section/subsection/clause                  | "Section"     | `^CHAPTER\s+([IVX\d]+)`, `^\d+\.\s+[A-Z][A-Z\s]+$` (numbered ALL-CAPS section), `^\d+\.\d+\s` (subsection)                                              |
| `generic`    | document/part/chapter/section/subsection/clause                  | "Section"     | bold-line detection, ALL-CAPS line, numbered `^\d+(\.\d+)*\s+[A-Z]` outline numbering                                                                  |

Patterns are seeds. The acceptance gate plus golden tests will drive iteration against the real corpus.

## Schema changes (migration 017)

```sql
ALTER TABLE sources ADD COLUMN doc_type         TEXT;
ALTER TABLE sources ADD COLUMN doc_type_source  TEXT;
ALTER TABLE sources ADD COLUMN structure_status TEXT;
ALTER TABLE sources ADD COLUMN structure_importer_version INTEGER NOT NULL DEFAULT 0;
```

| Column | Domain | Purpose |
|---|---|---|
| `doc_type` | `'constitution' \| 'act' \| 'rules' \| 'manual' \| 'generic' \| NULL` | Resolved profile per source |
| `doc_type_source` | `'auto' \| 'llm' \| 'user'` | Provenance — `user` is never overwritten by auto-detection |
| `structure_status` | `'structured' \| 'failed' \| 'skipped'` | Outcome of Step 4–6. `failed` = fell back to size-based |
| `structure_importer_version` | INTEGER | Version of the importer that last processed this source. Bumping the constant in code triggers re-index on next launch. |

No changes to `chunks`, `chunk_embeddings`, `graph_nodes`, `graph_edges`. All four already accept the full storage vocabulary.

## Auto-reindex mechanism

Mirrors Feature 004's startup re-extract hook (`services/indexer.rs::startup_reextract_hook`).

```rust
const STRUCTURED_IMPORTER_VERSION: u32 = 1;

for src in sources_below_version(STRUCTURED_IMPORTER_VERSION) {
    let force_profile = if src.doc_type_source == Some("user") {
        Some(src.doc_type.clone())  // honor manual choice
    } else {
        None
    };
    match run_structured_pipeline(src, force_profile) {
        Ok(_) => bump_version(src, STRUCTURED_IMPORTER_VERSION),
        Err(e) => log_and_keep_old_chunks(src, e),
    }
}
```

- Migration 017 sets `structure_importer_version = 0` for all existing rows → first launch after upgrade re-indexes the full Sindh corpus (~1–2 min)
- One source per transaction; failure leaves source at old version, surfaces in UI
- Embeddings + NER are re-run by the existing post-chunking pipeline (no special-casing here)

## Failure semantics — graceful degradation, never silent

| Step | Failure | Behavior |
|---|---|---|
| 1 Extract | Empty / corrupt PDF | Existing Feature 004 handling |
| 2 Normalize | Bad UTF-8 | Skip normalization for the affected span, continue |
| 3 Detect profile | No profile ≥ min_confidence | Use `generic` profile |
| 3 Detect profile | LLM tiebreaker timeout (>5s) | Use higher-scoring regex result, log warning |
| 4 Detect headings | Coverage < 10% of doc text | `structure_status='failed'`, fall back to size-based chunking |
| 5 structure_parser | Parser exception | As above |
| 6 Store | DB constraint violation | Rollback transaction, keep previous chunks, log to `execution_log` |

**Contract**: a source is always either fully structured or in size-based fallback. Never half-structured. UI surfaces `structure_status='failed'` so the user knows to retag or wait for a profile update.

## Testing strategy (three layers)

### Layer 1 — unit tests (~25 tests)
- `detection_patterns`: 4 profiles × 3 positive + 3 negative snippets
- `heading_patterns`: 4 profiles × 3 positive + 3 negative snippets
- `text_normalizer::normalize_text`: ~10 golden input → output cases (known OCR misreads)
- `pick_profile`: decision-logic tests (tie-breaking, threshold, user-override)

### Layer 2 — IR golden tests
- One snapshot file per fixture, committed to `tests/fixtures/structured_indexing/`
- Each snapshot lists `(unit_type, unit_label, char_count)` per node
- Fixtures: 5 Sindh PDFs + `constitution_ir.json` (regression coverage for shipped feature)
- Diff-on-fail with a generated `.actual` file for easy inspection

### Layer 3 — acceptance gate
`cargo test --test structured_indexing_acceptance`:
- Every Sindh PDF gets `doc_type != 'generic'`
- Each gets >0 `graph_nodes` and >0 `graph_edges`
- Citation strings for known-good chunks match expected (e.g., "Rule 9", "Section 3")
- Negative test: a non-legal PDF gets `doc_type='generic'` and still produces a coherent chapter → section → leaf tree
- Runs **without** LLM (regex-only tiebreaker for determinism)

The acceptance gate is the definition of done.

## Risks and mitigations

1. **Profile-regex iteration cost** — first-pass patterns will mis-classify or under-detect. Mitigation: ship behind `[experimental]` flag, iterate against Sindh corpus + IR golden tests, flip on by default after acceptance gate is stable.
2. **NER drift** — re-cut chunks invalidate existing `entity_mentions` for affected sources. Counts shift slightly. Mitigation: existing pass-2 NER scheduler picks up re-cut sources automatically; no manual action. Acceptance gate measures the delta and fails if entity counts collapse.
3. **OCR cleanup over-correction** — dictionary fix-ups could damage genuinely rare-spelled words. Mitigation: dictionary entries require ≥2-letter Levenshtein distance from any common-English word; dictionary capped at <50 entries v1, every entry reviewed manually before merge.

## What this design does NOT change

- Constitution path continues to use the structured-IR import (no regression)
- Embedding model, vector dim, retrieval stack, reranker, all chat-side code — untouched
- `chunks`, `chunk_embeddings`, `graph_nodes`, `graph_edges` schemas — no migrations
- Feature 004 three-tier extractor — no changes

## Open questions for implementation phase

- Confidence threshold value (`min_confidence`) — pick empirically after first run on Sindh corpus
- Whether `generic` profile should still try to detect chapters/sections, or always go flat — depends on what the first heading-detection run produces
- Whether `structure_status='failed'` should auto-retry on next app launch or require user action — defer to feature flag period
