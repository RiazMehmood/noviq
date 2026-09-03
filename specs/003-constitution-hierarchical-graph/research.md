# Phase 0 Research — Constitution v1

All NEEDS CLARIFICATION items resolved. Decisions consolidated from the design
discussion in `docs/research/legal-corpus-hierarchy-architecture.md`.

## D1 — Source acquisition

**Decision**: Acquire the constitution from **pakistani.org/pakistan/constitution**
(structured HTML, per-Part/Chapter pages; article number + marginal note +
clauses `(1)(2)` + sub-clauses `(a)(b)` + `Provided that` + `Explanation-I` all
typographically distinct; amendments incorporated through the 27th). Cross-check
article text/count against the National Assembly official PDF (na.gov.pk).

**Rationale**: The `constitution.pdf` extraction lost ~93 % of article headings.
A clean structured HTML source gives explicit Part/Chapter/Article/Clause
boundaries — no OCR repair needed.

**Alternatives rejected**: na.gov.pk PDF (same extraction problem); CommonLII
(403 on 2026-05-21); OCR-repairing the existing PDF (expensive, lossy).

**Constraint**: the importer is a **dev-time/offline tool** producing a bundled
static structured asset — the shipped app makes no network calls (Principle I).

## D2 — Graph storage

**Decision**: Graph as two SQLite tables in `echo.db` — `graph_nodes`,
`graph_edges` — traversed with bounded recursive CTEs.

**Rationale**: Constitution Principle IV mandates a single `echo.db` and forbids
separate database files. Scale analysis: production (~2,000 docs) ≈ ~1M nodes /
~3M edges, but retrieval-time graph use is **local neighbourhood expansion**
(1–2 hops from ~20 seeds) — cost is independent of total corpus size. A
composite index on `graph_edges(from_node, edge_type)` makes 1-hop expansion
~20 index seeks regardless of graph size.

**Alternatives rejected**: CozoDB / KuzuDB / Oxigraph — a dedicated store
violates Principle IV (would need a constitution amendment + ADR), adds a second
storage engine and WAL/lock-contention risk, and only pays off for whole-graph
analytics that legal RAG retrieval does not perform.

## D3 — Leaf granularity

**Decision**: Embed/retrieve/rerank leaf = **sub-clause** `(a)/(b)/…`, with the
article heading + clause chapeau prepended and any proviso/explanation kept
attached in-leaf. Article-level and Part-level chunks also embedded for routing.

**Rationale**: Sub-clause is the finest citable unit and best for deep
extraction. Prepending the chapeau prevents meaning loss; keeping provisos
in-leaf prevents negation inversion (failure F3).

**Alternatives rejected**: Article-level (mega-article mush); clause-level
(insufficient for the user's deep-extraction requirement); semantic chunking
(splits enumerated lists, merges provisos).

## D4 — Structure-aware parsing

**Decision**: A `constitution` **doc-type profile** (level vocabulary + heading
patterns) drives a structure-aware parser over the clean HTML intermediate,
replacing the regex-on-dirty-text heading detector for this corpus.

**Rationale**: Generalises to v2 Act/Rules profiles. The clean HTML source has
deterministic structural markers — no LLM disambiguation pass needed.

## D5 — Build on the Tier 3 branch

**Decision**: Build on `feature/tier3-hierarchical-retrieval`. Reuse the
hierarchical schema (`parent_chunk_id`, `unit_type`, `unit_path`,
`domain_metadata`), chunker, reranker, grounding, and `query_router` wiring.

**Rationale**: The Tier 3 plumbing is correct and tested; v1 fixes the *input*
and adds the graph layer + citation fast-path. Smallest viable diff.

## Open (deferred to v2)

- OCR/layout-aware extraction engine for the Sindh govt corpus.
- Sindh Act/Rules doc-type profiles.
