# Implementation Plan: Constitution v1 — Hierarchical + Graph Retrieval

**Branch**: `feature/tier3-hierarchical-retrieval` (SpecKit feature `003-constitution-hierarchical-graph`) | **Date**: 2026-05-21 | **Spec**: `specs/003-constitution-hierarchical-graph/spec.md`
**Input**: Feature specification + `docs/research/legal-corpus-hierarchy-architecture.md` §7

## Summary

Replace the degraded flat indexing of the Constitution of Pakistan (current
result: 1 document / 11 sections / 253 leaves, ~93 % of leaves under 6
mislabeled sections) with a structure-aware **hierarchical tree + graph** built
from a clean HTML source. Leaves are indexed to **sub-clause** granularity; an
in-`echo.db` graph (`graph_nodes` + typed `graph_edges`) supports
cross-reference / amendment / non-obstante expansion at retrieval time. A
citation fast-path answers article-numbered queries deterministically. Builds on
the existing Tier 3 branch — reuses schema, chunker, reranker, grounding,
`query_router`.

## Technical Context

**Language/Version**: Rust (stable), crate `echo_ai_lib`. Frontend (SolidJS) untouched in v1.
**Primary Dependencies**: `sqlx` (SQLite), `fastembed-rs` (Nomic 768-d, sqlite-vec/vec0), FTS5, existing ONNX reranker (`ort`), `regex_lite`, an HTML-parsing crate for the dev-time importer (`scraper` or similar — verify availability).
**Storage**: single `echo.db`. New tables `graph_nodes`, `graph_edges`; extended `chunks.unit_type` vocabulary. No new database file.
**Testing**: `cargo test`; `~/Desktop/db_inspection` harness (scripts 01–08 + new 09 citation-accuracy, 10 graph-integrity); `scripts/reranker_eval` for the F1–F5 retrieval gate.
**Target Platform**: Tauri 2 desktop (Linux/Windows), offline.
**Project Type**: single (Rust backend service modules).
**Performance Goals**: p95 retrieval < 2 s (Tier 2 budget); graph 1-hop expansion < 5 ms; full constitution index in minutes.
**Constraints**: offline-always — source acquisition is a dev-time tool, not runtime network; additive migrations only; smallest viable diff.
**Scale/Scope**: v1 ≈ 1.8k graph nodes / ~4k edges; production design target ≈ 1M nodes / 3M edges.

## Constitution Check

*GATE: passed (post-research). Re-checked after design — still passes.*

| Principle | Status | Note |
|---|---|---|
| I. Privacy-First, Offline-Always | ✅ | Source import is a **dev-time/offline tool** producing a bundled static asset; shipped app makes no network calls. |
| II. Tiered Intelligence | ✅ | Citation fast-path = deterministic lookup before any LLM tier; strengthens tier-cheapest-first. |
| III. Single Model Slot | ✅ | No new model. Reuses the existing embedder + reranker. |
| IV. SQLite Single Source of Truth | ✅ | Graph as tables **inside `echo.db`**; CozoDB/separate-file rejected for this reason (research D2). |
| V. Pending Actions Queue | ✅ | Indexing is not a DANGEROUS tool call. N/A. |
| VI. Persona-as-Config | ✅ | N/A — no agent persona change. |
| VII. Hardware-Adaptive | ✅ | Graph tables + recursive CTEs are light; no tier-gated feature. |
| VIII. Incremental Feature-App Delivery | ✅ | v1 (constitution-only) is independently shippable and testable. |
| Dev constraint: additive migrations | ✅ | New schema migration adds tables + extends a CHECK vocabulary; no drops. |

No violations → **Complexity Tracking empty**.

## Project Structure

### Documentation (this feature)

```text
specs/003-constitution-hierarchical-graph/
├── plan.md          # This file
├── spec.md          # Feature spec
├── research.md      # Phase 0 — decisions D1–D5
├── data-model.md    # Phase 1 — node/edge/leaf model + module interfaces
├── quickstart.md    # Phase 1 — build & validate v1
└── tasks.md         # Phase 2 — created later by /sp.tasks
```

No `contracts/` directory: v1 exposes **no external REST/GraphQL API** — it is
internal Rust indexing/retrieval. Module-level interfaces are specified in
`data-model.md` instead.

### Source Code (repository root)

```text
src-tauri/src/
├── db/
│   ├── schema.rs                  # + migration 014: graph_nodes, graph_edges, unit_type vocab
│   └── repositories/
│       ├── chunks.rs              # extend unit_type vocabulary
│       └── graph.rs               # NEW — graph_nodes/graph_edges repo + bounded-CTE traversal
├── services/
│   ├── doc_profiles/              # NEW — doc-type profiles (level vocab + patterns)
│   │   └── constitution.rs        # NEW — the `constitution` profile
│   ├── structure_parser.rs        # NEW — clean-HTML IR → L0–L5 tree (profile-driven)
│   ├── graph_builder.rs           # NEW — tree + clause text → graph edges
│   ├── chunker.rs                 # extend chunk_tree for sub-clause leaves + chapeau prepend
│   ├── indexer.rs                 # wire the constitution import path
│   └── query_router.rs            # citation fast-path + 1-hop graph expansion
└── lib.rs                         # register new repos/services

tools/constitution_import/         # NEW — dev-time/offline source importer (separate bin)

tests/
├── structure_parser_test.rs       # NEW
├── graph_builder_test.rs          # NEW
└── constitution_v1_acceptance.rs  # NEW — A1–A4 acceptance
```

**Structure Decision**: single-project Rust backend. The source importer is a
**separate dev-time binary** under `tools/` so it never ships in the app bundle
(Principle I). All runtime code stays in `src-tauri/src/`.

## Implementation Phases (S1–S7)

Dependency-ordered. Each stage has explicit acceptance before the next begins.

### S1 — Source importer (dev-time)
- **Goal**: clean structured intermediate representation (IR) of the constitution.
- **Work**: `tools/constitution_import/` — fetch pakistani.org per-Part/Chapter
  HTML, parse the typographic markers, emit a deterministic IR (JSON tree:
  parts → chapters → articles → clauses → sub-clauses, with marginal notes,
  provisos, explanations, amendment annotations). Cross-check article count
  against na.gov.pk.
- **Accept**: IR contains ~12 Parts, ~290 Articles, every article has a marginal
  note; manual spot-check of 10 articles against the official PDF.

### S2 — Doc-type profile + structure-aware parser
- **Goal**: IR → L0–L5 `DocNode` tree.
- **Work**: `services/doc_profiles/constitution.rs` (level vocabulary + heading
  patterns); `services/structure_parser.rs` consumes the IR + profile → tree.
- **Accept**: unit tests — tree depth, node counts per level, provisos attached
  to parent clauses.

### S3 — Schema + persistence
- **Goal**: persist the tree.
- **Work**: migration 014 — `graph_nodes`, `graph_edges`, extend
  `chunks.unit_type` vocabulary to `part/chapter/article/clause/subclause`;
  composite index `graph_edges(from_node, edge_type)` and `(to_node, edge_type)`.
  Extend `repositories/chunks.rs`; write the tree with `parent_chunk_id`,
  `unit_path`, citation metadata in `domain_metadata`.
- **Accept**: A1 structural check (db_inspection `07`/`08`) on a parsed tree.

### S4 — Graph builder
- **Goal**: populate `graph_nodes`/`graph_edges`.
- **Work**: `services/graph_builder.rs` — `contains` (tree), `cross_reference`
  (parse "Article N" mentions in clause text), `non_obstante` ("notwithstanding"
  detection), `amended_by` (from IR amendment annotations).
- **Accept**: A3 graph-integrity check (new script `10`).

### S5 — Multi-level embedding
- **Goal**: embeddings for retrieval.
- **Work**: extend `chunker.rs`/`indexer.rs` — emit sub-clause leaves with
  heading + chapeau prepended; embed leaves + article + part chunks via the
  existing embedder into sqlite-vec.
- **Accept**: chunks == chunk_embeddings; dim 768; no NaN/Inf (db_inspection 01–04).

### S6 — Retrieval pipeline
- **Goal**: wire v1 retrieval.
- **Work**: `query_router.rs` — citation fast-path (regex article/clause in
  query → deterministic metadata lookup, bypass reranker); hybrid BM25/FTS5 ⊕
  vector RRF over sub-clause leaves; reranker; small-to-big expansion to article;
  1-hop graph expansion via `repositories/graph.rs` bounded CTE; clause-precise
  citations in the trace.
- **Accept**: A4 — F1–F5 re-run via `scripts/reranker_eval`.

### S7 — Acceptance harness
- **Goal**: the v1 gate.
- **Work**: new db_inspection scripts `09` (citation accuracy, A2) and `10`
  (graph integrity, A3); `tests/constitution_v1_acceptance.rs`; consolidated
  A1–A4 run + CHANGELOG_DEV.md entry.
- **Accept**: A1–A4 all pass → v1 done; unblocks v2 (OCR).

## Phase planning note

- **Phase 0** (research.md) — complete; decisions D1–D5 resolved.
- **Phase 1** (data-model.md, quickstart.md) — complete.
- **Phase 2** (tasks.md) — generated by `/sp.tasks`, NOT by this command.

## Complexity Tracking

*No constitution violations — section intentionally empty.*
