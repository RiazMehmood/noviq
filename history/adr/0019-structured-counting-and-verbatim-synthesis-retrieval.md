# ADR-0019: Structured Counting and Verbatim Synthesis for Retrieval

> **Scope**: How the assistant answers two query classes it currently answers
> poorly — *count/list* ("how many rules are in the SPPRA Rules?") and
> *whole-doc/topic synthesis* ("what does the corpus say about X?"). Groups both
> because they share one root cause (the graph is built at ingest but barely used
> at answer time) and one governing principle (verbatim legal fidelity).

- **Status:** Proposed
- **Date:** 2026-07-11
- **Feature:** GraphRAG & graph-usage audit (follow-up to ADR-0007)
- **Context:** A 2026-07-11 code+DB audit found the system is "GraphRAG-lite":
  `graph_nodes`/`graph_edges` are built at ingest (`services/graph_builder.rs`)
  but retrieval touches the graph in exactly one place — a single-hop citation
  expander behind the citation fast-path (`query_router.rs:2921`). Two user-facing
  failure modes fall out of this: (a) counting/list questions are routed through
  the LLM, which cannot be trusted to count; (b) whole-document and corpus-wide
  "what does it say about X" questions have no structure-aware recall and degrade
  to top-K chunk soup. Prod-DB evidence (`echo`, 18 sources / 13 graph-covered):
  `graph_nodes.node_type` is a purely structural hierarchy level
  (`document|part|chapter|article|clause|subclause`), `sources.doc_type` is the
  semantic classifier (`act|rules|manual|generic`), and the human-facing unit word
  ("rule" vs "section" vs "article") lives in each `DocProfile.level_vocab`, not in
  `node_type`. A naive `COUNT(*)` over Constitution articles returned 332; the
  parser emits duplicate nodes for multi-chunk provisions, so only
  `COUNT(DISTINCT citation)` = 283 ≈ the real 280 base articles (+ inserted
  articles such as 10A). Departments/table aggregates are **not** in any structured
  table (the `entities` table holds only 19 dates + 2 refs); that data is trapped
  in Schedule-I tables and is out of scope here.

<!-- Significance checklist:
     1) Impact: yes — introduces a deterministic non-LLM answer path + an ingest-time
        summarization layer; long-term retrieval-architecture consequence.
     2) Alternatives: yes — LLM counting, full community-detection GraphRAG,
        summaries-as-answer all considered.
     3) Scope: yes — cross-cutting across intent routing, ingest, and answer contract. -->

## Decision

Two coordinated changes, both keeping the graph **inside PostgreSQL** (ADR-0007 /
ADR-0015, single source of truth — no new store, no Neo4j).

**1. Deterministic structured-count path (Track 1, build now).**
A new `Intent::Count` short-circuits *before* retrieval and *before* the LLM into
SQL over data we already have:
- **Corpus-level** ("how many acts/rules/manuals/documents"): `GROUP BY doc_type`
  over `sources` within the active scope.
- **Within-document** ("how many rules/sections/articles/clauses in doc X"):
  translate the officer's unit word → `node_type` via the target document's
  `DocProfile.level_vocab` (e.g. a `rules` doc: "rule" → `level_vocab[3]` →
  `node_type='article'`), then `COUNT(DISTINCT citation)` over `graph_nodes` joined
  through `chunks` to the scoped `source_id`s.
- The count answer is assembled by **code, not a model** — exact integer plus a
  structural breakdown, each figure carrying a citation to its source document.
  `COUNT(DISTINCT citation)` (never `COUNT(*)`) is mandatory to absorb the
  duplicate-node data bug.

**2. Summaries-as-index / verbatim-as-answer synthesis (Track 2, design now, ADR-gated).**
Pre-bake RAPTOR-style per-document hierarchical summaries at ingest (summarize
clauses→articles→parts→document) stored as auxiliary nodes. These summaries are a
**recall/navigation layer only**: at query time they identify *which* provisions
across a document/corpus are relevant, the system then pulls the **original chunk
text** for those provisions, and the answer quotes them **verbatim with citations**.
No summary text is ever shown to the user. This preserves legal fidelity (the
officer sees the real rule, not a paraphrase) and reuses the existing map-reduce and
citation machinery.

**Out of scope / parked:** entity- and table-row counts (e.g. "sum of all
departments") depend on the Schedule-I table-chunking fix and an NER backfill; they
are not answerable from `graph_nodes` and are deferred.

## Consequences

### Positive

- **Exact, instant, model-free counts.** No hallucinated numbers; latency drops from
  a full inference turn to a single SQL query; answers are verifiable against ground
  truth (Constitution → 283 ≈ 280).
- **Reuses existing data.** No migration for Track 1: `sources.doc_type`,
  `graph_nodes`, and `DocProfile.level_vocab` already exist.
- **Legal fidelity preserved for synthesis.** Verbatim-as-answer keeps the "quoter,
  not generator" contract (see the Qwen fine-tune lesson) even while gaining
  whole-doc recall.
- **Stays within Constitution §IV / ADR-0007.** Graph and summaries live in Postgres;
  no second engine to bundle in the offline desktop app.

### Negative

- **Count trust depends on `doc_type` correctness.** At least one live
  misclassification (SPPA-2009 tagged `manual`) must be fixed before corpus counts
  are trustworthy; `doc_type` is auto-detected and can drift.
- **Count coverage = graph coverage.** Only 13 of 18 sources have a structure tree;
  the 5 uncovered docs cannot be counted structurally and must degrade gracefully
  ("this document isn't structurally indexed").
- **Unit-word ambiguity.** "Rules" can mean a document type or a within-doc unit;
  the router disambiguates by scope + naming and must fail safe (ask/clarify) rather
  than answer the wrong question.
- **Duplicate-node latent bug remains.** `COUNT(DISTINCT citation)` masks but does not
  fix `graph_builder.rs` emitting one node per chunk occurrence; a separate cleanup is
  warranted.
- **Track 2 adds ingest cost.** Per-document bottom-up summarization runs the LLM at
  ingest time; acceptable for a one-time/offline indexing pass but non-zero.

## Alternatives Considered

- **Let the LLM count (status quo).** Rejected: LLMs produce confident-but-wrong
  numbers; this is the failure being fixed. Counting is a database operation.
- **Full Microsoft-style community-detection GraphRAG (Leiden/Louvain over
  `graph_edges`, per-community summaries).** Rejected for now: overkill at this graph
  size (6.6k nodes, 13 shallow docs), adds a clustering dependency, and its global
  summaries would still need to point back to verbatim provisions anyway. RAPTOR-style
  per-doc hierarchy gives the summary→source pointer for free via `contains` edges.
- **Summaries-as-answer (return the LLM summary text).** Rejected on explicit
  requirement: legal answers must be the original provision text, not a paraphrase.
- **Neo4j / dedicated graph store.** Rejected: breaks single-source-of-truth, heavy
  op tax for an offline Tauri desktop app, and the real gap is *using* the graph, not
  the store. If true global GraphRAG is ever needed, do it on Postgres (Apache AGE /
  pgvector), algorithm-first.

## References

- Related ADRs: ADR-0007 (hierarchical + graph document model), ADR-0015 (Postgres as
  single source of truth), ADR-0009 (query decomposition), ADR-0016 (unified chat
  assistant architecture)
- Evidence: 2026-07-11 prod-DB audit (`echo` on VM); memory
  `project_graphrag-and-graph-usage-audit`; `retrieval audit.txt` (master audit brief,
  §GraphRAG / Part 7 / Part 16)
- Code: `services/graph_builder.rs`, `services/structure_parser.rs:51` (`unit_type`),
  `services/doc_profiles/*` (`level_vocab`), `services/intent_router/*`,
  `db/repositories/graph.rs`
