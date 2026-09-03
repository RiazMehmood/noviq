# ADR-0020: Graph-Augmented Retrieval on Postgres (own GraphRAG, not Neo4j)

> **Scope**: Documents the decision cluster for adding graph traversal to retrieval — the store (existing Postgres graph, not a new engine), the edge model (generalized provision references + corpus cross-document linker), the retrieval integration (seed-and-expand arm), and the amendment-awareness treatment. Grouped because they only deliver value together.

- **Status:** Accepted
- **Date:** 2026-07-12
- **Feature:** GraphRAG — graph-augmented retrieval (branch `feat/document-full-summary`)
- **Context:** Pure hybrid retrieval (vector ANN + FTS5, `ECHO_HYBRID=union`) retrieves passages that are *semantically similar to the query*, not passages that are *legally connected to the answer*. In a legal corpus the answer to "what do I do about X" frequently depends on provisions that are lexically dissimilar to the question — an amendment ("substitute in sub-rule (2)…"), a non-obstante override ("notwithstanding anything in…"), a cross-referenced rule, or a parent enabling Act. These are structurally invisible to similarity search, so the system could return a confident-but-incomplete answer. A legal-document graph already existed in Postgres (`graph_nodes`/`graph_edges`, ADR-0007) but a live audit showed retrieval barely used it, and the relational overlay was effectively empty outside the Constitution.

## Decision

Build graph-augmented retrieval **on the existing bundled Postgres graph** — **do not adopt Neo4j or any second datastore**. The decision cluster:

- **Store — Postgres, not Neo4j.** Traverse `graph_edges` with the existing bounded recursive-CTE primitive (`graph::expand`, hard-capped at 3 hops). No new engine, no dual-store sync, no §IV/ADR-0015 rewrite. If Cypher/community-detection is ever needed, evaluate **Apache AGE inside Postgres** first (algorithm before engine).
- **Edge model — generalized + corpus-aware.**
  - Within-document overlay (`graph_builder`): the reference extractor was generalized from "Article N" only to the full legal-citation vocabulary (`article|section|rule|regulation|clause|chapter|part|schedule` + number), with references and stored citations both normalized to a lookup key (`Rule 5(6)` / `Rules 3` → `rule5`/`rule3`). Arabic-only in v1 (matching bare romans would mint false legal edges — worse than none).
  - Cross-document overlay (`graph_linker`): a post-ingest corpus pass links a subordinate instrument to its enabling Act (Rules → Act), resolving a named instrument's title to another source, then (b) a provision named after the title → that provision's node, else (a) the target's document node. **No fuzzy matching** — a title must match distinctive multi-word tokens.
  - Regeneration without re-ingest: `rebuild_overlay` + `POST /api/rebuild_graph` regenerate the full overlay from persisted chunk text + nodes, so a corpus indexed before this work is upgraded without re-OCR/re-embed.
- **Retrieval integration — additive, flag-gated seed-and-expand arm.** After the hybrid merge, the top-K merged chunks seed `graph::expand` over `cross_reference/non_obstante/amended_by`; reached provisions are scope-clamped and union-merged into the candidate pool (tier `hybrid+graph`). The downstream reranker/context builder re-scores, so an off-topic neighbour drops out. Gated by `ECHO_GRAPH_EXPAND` (default OFF), a safe A/B like `ECHO_HYBRID`.
- **Amendment awareness — annotation, not an edge.** The `amendment_events` audit showed every amending instrument (amendment Acts, gazette notifications) is *external* to the corpus, so an `amended_by` graph edge has no in-corpus target. Instead, provisions reaching the LLM context that have recorded amendment events are annotated (`[⚠ Amendment history: … may be superseded …]`), reusing F013/F015 provenance. Gated by `ECHO_AMEND_NOTES` (default OFF). `amended_by` remains in the traversal edge set so it free-activates if a future corpus ever contains an amending instrument.

## Consequences

### Positive

- **Completeness/correctness gain where similarity is blind** — amendments, non-obstante overrides, cross-references, and Rules→Act links now reach the answer context. Verified live (trace `515b824f`): tier `hybrid+graph`, graph expansion folded in a connected provision, amendment annotation fired.
- **No governance breach** — stays a single bundled Postgres store (Principle I / §IV / ADR-0015 intact). No JVM, no dual-write sync, no offline-bundling problem, no constitution amendment.
- **Overlay went from constitution-only to corpus-wide** — 116 → 616 `cross_reference` + 18 `non_obstante` across 13 documents; cross-doc edges are clean and correct (SPPRA→SPP Act, Compendium→SLGA, Removal→E&D) with no junk, validating the no-fuzzy resolver.
- **Cheap regeneration** — `rebuild_graph` rebuilds the whole overlay in seconds from existing data; no full re-index.
- **Reversible, non-invasive rollout** — every new behaviour is flag-gated OFF; deploying changed nothing until the flags were flipped.

### Negative

- **Completeness is only as good as the extracted edges** — deterministic reference/title matching misses relationships that are implicit (a bare "the Act", legally-interacting provisions that never cite each other). Mitigated by keeping the graph *additive* to vector/FTS (hybrid, never a replacement), so a missing edge degrades to today's behaviour.
- **Hop/junk risk** — unbounded expansion could replace similarity-junk with traversal-junk; mitigated by small seed-K, 1-hop default, a K cap, and the downstream reranker.
- **Two more flags + an admin endpoint** to operate; overlay must be rebuilt after a re-index (documented runbook).
- **Latency win is secondary** — the measured bottleneck is generation (~14 tok/s vGPU), not prefill, so the primary payoff is correctness, not speed. *(See Addendum 2026-08-13 — the tok/s figure was a software defect, not a hardware ceiling. The conclusion of this bullet is unchanged.)*

## Alternatives Considered

- **Neo4j (dedicated graph engine).** Rejected. Fixes none of the current pains (summary quality, routing, parse mislabels, latency, counting are not graph problems); the one genuine fit (corpus-wide synthesis) is a non-need today. Even in the current client-server mode it is a second stateful service to run, secure, and keep in dual-write sync with Postgres, with no performance driver at our scale (~14 shallow instruments; Postgres recursive CTEs already do the traversal). Full eval: `C:\Users\hp\.claude\plans\luminous-honking-flute.md`.
- **Classic GraphRAG (Leiden/Louvain community detection + community summaries).** Rejected for now. Wrong shape for a corpus of single legal instruments — communities ≈ the documents themselves; the existing RAPTOR-style summary index (`graph_summary`, ADR-0019) already covers most synthesis value.
- **`amended_by` as a real graph edge.** Rejected on data — every amending instrument is external to the corpus, so it would produce zero edges. Replaced by the annotation approach.
- **Fuzzy / embedding-assisted cross-document linking.** Rejected — a wrong legal edge is worse than a missing one; the linker requires distinctive title-token matches and skips otherwise.
- **Apache AGE (openCypher inside Postgres).** Deferred, not rejected — the correct next step *if* corpus-wide synthesis or heavy multi-hop analytics becomes a real requirement. Algorithm before engine, on the existing store.

## References

- Feature Spec: branch `feat/document-full-summary` (no standalone spec; commits `0a6987fd`, `4f0a7c03`, `7a82fa7a`, `ad0e80dd`)
- Implementation Plan / Neo4j evaluation: `C:\Users\hp\.claude\plans\luminous-honking-flute.md`
- Related ADRs: ADR-0007 (graph-in-Postgres), ADR-0015 (Postgres single source of truth), ADR-0019 (structured counting + verbatim synthesis / summary index)
- Evaluator Evidence: `CHANGELOG_DEV.md` entries latest9–latest12; live prod query trace `515b824f` (tier `hybrid+graph`, graph-expansion + amendment-annotation steps); overlay audit 616 `cross_reference` + 18 `non_obstante` over 13 docs

## Addendum — 2026-08-13: the "~14 tok/s vGPU" figure was a software defect

The Consequences → Negative bullet on generation latency cited **~14 tok/s** and
attributed it to the A40-12Q being a time-sliced quarter of a GPU. **That
attribution was wrong.** Measured 2026-08-12:

- The A40-12Q partitions the **frame buffer** (12 of 48 GB), not compute. The
  profile reaches **52–61% of the full card's FP16 peak** and **564 GB/s**
  (81% of the A40's 696 GB/s spec) — neither is possible under a ¼-compute cap.
- The real cause was passing `--quantization awq` **explicitly** to vLLM, which
  suppresses its automatic upgrade to the Marlin kernel. Correcting the flag took
  decode from **13.75 → 81.32 tok/s** (5.9×) with no code change; n-gram
  speculative decoding took a production-shaped request to **3.6 s** end-to-end,
  from 36.2 s.

**The decision recorded in this ADR is unaffected** — generation still dominates
prefill, so graph augmentation's payoff remains correctness rather than speed.
What changes is the *framing elsewhere*: answer latency is no longer evidence for
"buy a bigger GPU". GPU **memory** remains a genuine constraint on context window,
model size and concurrency; GPU **compute** does not constrain answer speed.

The original bullet is left in place as written; this addendum supersedes its
parenthetical figure.
