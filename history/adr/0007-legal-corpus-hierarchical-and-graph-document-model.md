# ADR-0007: Legal Corpus Hierarchical and Graph Document Model

> **Scope**: Document decision clusters, not individual technology choices. Group related decisions that work together.

- **Status:** Accepted
- **Date:** 2026-05-21
- **Feature:** 003-constitution-hierarchical-graph
- **Context:** Indexing `constitution.pdf` through the Tier 3 pipeline produced 1 document / 11 sections / 253 leaves, with 6 section labels absorbing 93 % of all leaves (e.g. `Article 69` = 48 leaves spanning ~60 articles). Section labels mislabel ~93 % of leaves, so citations are wrong and the reranker cannot disambiguate articles — the root cause of incorrect constitutional answers (failures F1–F5). The corpus also extends beyond the constitution to a Sindh government document set (Acts, Rules, Manuals, Notifications, Schedule-amendments) sharing the same numbered-clause structure. A single, generalizable decomposition + retrieval model is needed.

## Decision

Adopt an integrated **profile-driven hierarchical + graph document model** for the legal/government corpus:

- **Hierarchy:** `Document → Part → Chapter → Article → Clause → Sub-clause`, persisted in the existing Tier 3 `chunks` table (extended `unit_type` vocabulary, `parent_chunk_id`, `unit_path`).
- **Doc-type profiles:** the structure-aware parser is parametrized by a per-document-type profile (level vocabulary + heading patterns) — `constitution` first; `Act`, `Rules`, etc. later — so the model generalizes across the corpus rather than being constitution-specific.
- **Leaf granularity = sub-clause**, with the article heading + clause chapeau prepended and provisos/explanations kept attached in-leaf (never split from the rule they modify).
- **Graph layer stored as SQLite tables in `echo.db`** — `graph_nodes` + typed `graph_edges` (`contains`, `cross_reference`, `non_obstante`, `amended_by`), traversed with bounded recursive CTEs. **No dedicated graph engine** (CozoDB/KuzuDB/Oxigraph).
- **Source acquisition:** clean structured HTML where it exists (constitution → pakistani.org, dev-time/offline import); layout-aware OCR for the rest of the corpus (v2).
- **Retrieval:** citation fast-path (deterministic article/clause lookup) → hybrid BM25/FTS5 ⊕ vector RRF → reranker → small-to-big expansion → 1-hop graph expansion → clause-precise citations.

## Consequences

### Positive

- Article/clause-precise retrieval and **correct citations** — fixes the root cause of F1/F2/F4.
- Graph as SQLite tables honours **Constitution Principle IV** (single `echo.db`, no separate database files) — no second storage engine, one backup/migration story, no constitution amendment required.
- Retrieval-time graph cost is **local neighbourhood expansion** (1–2 hops from ~20 seeds) — independent of total corpus size; scales to the production target (~1M nodes / ~3M edges) with composite edge indexes.
- Profile-driven design generalizes to the Sindh govt corpus and beyond (contracts, standards) with no engine change.
- Builds on the existing Tier 3 schema/chunker/reranker — smallest viable diff.

### Negative

- SQLite recursive CTEs are unsuited to **whole-graph analytics** (PageRank, community detection); such analytics, if ever needed, must be precomputed offline and materialized as node columns.
- Graph writes concentrate more load on the single `echo.db` + WAL — requires careful transaction batching (this project has prior indexer pool-deadlock history).
- Each doc type needs a profile authored + validated — upfront effort per corpus type.
- Sub-clause granularity multiplies node/embedding count (~3–5k leaves for the constitution alone).

## Alternatives Considered

- **Dedicated embeddable graph engine (CozoDB / KuzuDB / Oxigraph).** Rejected: violates Constitution Principle IV (separate database file → requires a constitution amendment); adds a second storage engine and WAL/lock-contention risk; its advantage (whole-graph analytics, deep recursive reasoning) is capability legal RAG retrieval does not use.
- **Article-level leaves (no sub-clause split).** Rejected: large articles (199, 232, 260) become averaged "mush" the reranker cannot discriminate.
- **Fixed-token or semantic chunking.** Rejected: ignores legal boundaries — severs chapeau from clauses, merges/splits provisos, causing negation inversion (F3).
- **Constitution-specific (non-generic) model.** Rejected: the corpus is mostly govt documents sharing the same structure; a generic profile-driven model avoids a rewrite per doc type.
- **OCR-repairing the constitution PDF.** Rejected: expensive and lossy when a clean structured HTML source exists.

## References

- Feature Spec: `specs/003-constitution-hierarchical-graph/spec.md`
- Implementation Plan: `specs/003-constitution-hierarchical-graph/plan.md`
- Research / Design: `specs/003-constitution-hierarchical-graph/research.md`, `docs/research/legal-corpus-hierarchy-architecture.md`
- Related ADRs: ADR-0002 (Embedding Model Selection & Schema Strategy) — this feature extends that schema additively
- Constitution: Principle IV (SQLite as Single Source of Truth)
- Evaluator Evidence: `history/prompts/003-constitution-hierarchical-graph/0001-constitution-v1-hierarchical-graph-plan.plan.prompt.md`
