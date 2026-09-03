# Phase 0 Research: Agentic Deep Search

All decisions target reuse of existing code and a strictly bounded loop. Each item: Decision · Rationale ·
Alternatives.

## R1 — Which retrieval engine do sub-questions reuse?

**Decision**: Extract a reusable `retrieve_for_query(query_text, scope, deep_arms) -> Vec<SelectedChunk>` from
`query_router::route_scoped_with_override`, and call it **once per sub-question**. Do **not** call
`one_ask::research()`.

**Rationale**: `route_scoped_with_override` already implements exactly the retrieval we want per sub-question —
hybrid vector+FTS union, RRF merge, the `graph_expand` + `summary_recall` arms, relevance filtering, and
budget-bounded selection. `one_ask::research()` is drafting-coupled: it takes an `OneAskIntent`, a `Section`
skeleton, a `DocType`, and emits `ProvisionCard`s gated by per-section citation caps — none of which fit a Q&A
sub-question. The generalizable parts of `one_ask` are only the *low-level* helpers.

**Alternatives**:
- *Call `one_ask::research()` directly* (the spec seed) — rejected: bending drafting types (sections, caps,
  cards) to Q&A is more code and more risk than a clean extract.
- *Duplicate the retrieval block inside `deep_search`* — rejected: it would drift from the normal path; a single
  shared function keeps normal and deep retrieval identical.

**What `one_ask` still contributes**: `rrf_merge`, `relevant_hits` (merge/filter helpers) and, for R5, the
grounding primitives `quote_verifies` + `foreign_number_in_prose`.

## R2 — Coverage-check contract (the one genuinely new component)

**Decision**: A single **short, structured** LLM call that receives the draft answer + the list of
sub-questions (ids + text) and returns JSON: `per_subquestion: [{id, status: "answered"|"pending"}]`. `pending`
sub-questions drive round 2. Keep `max_tokens` small (~256); free-text JSON only (no llguidance — it has
crashed twice on this stack, per project history).

**Rationale**: The coverage judgment is a classification, not prose — it must be cheap so it doesn't blow the
latency budget. Emitting only ids + status (no re-rendered content) also means the coverage call **cannot
introduce hallucinated answer text** — it only routes retrieval.

**Bias-to-refetch (FR-007)**: on JSON parse failure, missing id, or an unrecognized status, default that
sub-question to **`pending`**. A wasted round-2 fetch is cheap (bounded to one round); a wrongly-dropped part is
a visible completeness failure.

**Alternatives**:
- *Confidence scores per sub-question* — rejected: adds tuning surface and tokens for no routing benefit over a
  binary answered/pending with a refetch bias.
- *Skip the draft, ask coverage on retrieved passages alone* — rejected: the draft is what reveals which parts
  the model could actually answer from round 1; it is the signal we want.

## R3 — Facet-expansion (LLM call #1)

**Decision**: Layer LLM expansion on top of `decompose::detect_and_split`. Prompt the model to return JSON
`{sub_questions: ["…", …]}` of **4–6** short sub-questions covering distinct facets/synonyms. **Always** prepend
the original query as sub-question #0 (the safety leg). Cap at 6; dedupe near-identical facets.

**Rationale**: `detect_and_split` already handles clean conjunctive splits deterministically; LLM expansion adds
the *facets the user didn't phrase* (synonyms, related provisions). Keeping the original query guarantees a bad
split cannot starve retrieval (FR-001).

**Fallback**: if the expansion call fails/returns unparseable/empty, fall back to `detect_and_split` output;
if that is also empty, use `[original_query]` — the loop then degrades to a single-facet near single-pass
(FR-012).

**Alternatives**:
- *Pure rule-based split (no LLM)* — rejected by the architect's decision (LLM facet expansion chosen); it
  misses unphrased facets.
- *Many sub-questions (>6)* — rejected: each adds a retrieval round-1 pass and dilutes the final window; 6 caps
  cost while covering typical multi-part legal questions.

## R4 — Cross-round assembly (dedup + budget packing)

**Decision**: Union round-1 ∪ round-2 chunks; dedup by chunk id (keep first-seen; a chunk re-found in round 2
does not double-count). Then pack to the answer window's context budget using `summary_builder::pack_windows`,
ordering so **passages that resolve previously-pending sub-questions are packed first**, then the rest by rank.

**Rationale**: Round 2 exists specifically to fill gaps, so its passages must be prioritized when the combined
set exceeds the window (FR-013). `pack_windows` already does budget-bounded greedy packing — reuse it.

**Alternatives**:
- *Just concatenate and truncate* — rejected: could drop the very round-2 passages the loop paid for.
- *Re-rank the whole union by similarity* — rejected: similarity re-ranking is what buries gap-filling
  passages (the original single-window failure mode).

## R5 — Grounding gate on the final answer (LLM call #3 output)

**Decision**: A small **general** verifier in `deep_search/verify.rs` that reuses `one_ask`'s
`quote_verifies` (a quoted span must appear in some retrieved passage) and `foreign_number_in_prose` (a figure
in the prose must appear in a retrieved passage) against the assembled `PassageSet`. Unverifiable citations are
dropped; a figure with no source is flagged. Guarantees ≥1 traceable citation (Constitution XI); if none
verify, the answer is downgraded to the grounded-fallback message rather than shipping ungrounded prose.

**Rationale**: These two primitives already encode the anti-hallucination checks that would have caught
"Department Zoonology"; generalizing them (off the drafting `ProvisionCard`/section model) is a small, testable
extract. Prefer refuse-or-flag over confident fabrication.

**Alternatives**:
- *Reuse `assemble_and_verify` wholesale* — rejected: coupled to `SectionId`/section caps/`ProvisionCard`.
- *Trust the synthesis prompt to stay grounded* — rejected: the immediately-preceding hallucination proves the
  prompt alone is insufficient.

## R6 — Flag, rollback, and A/B

**Decision**: Gate the loop behind `ECHO_AGENTIC_DEEP` (default **off** at first deploy). `deep=true` +
flag-on → agentic loop; `deep=true` + flag-off → the current one-pass deep (unchanged). This lets us A/B the
two deep paths on the same corpus before retiring one-pass deep. Retire the one-pass path (and the flag) only
after the A/B in `research/025-agentic-deep-search/` clears SC-001/002/003/006.

**Rationale**: The spec *replaces* one-pass deep, but a flag makes the switch reversible and measurable — matches
the established `ECHO_HYBRID`/`ECHO_GRAPH_EXPAND` A/B pattern and the "verify on a side branch first" practice.

**Alternatives**: *Hard replace immediately* — rejected: no rollback, no A/B evidence, higher blast radius on a
production path.

## R7 — Latency budget allocation (~65 s target)

**Decision**: Allocate roughly — expand #1 ≤ 5 s (small `max_tokens`), retrieval round 1 ≈ 3–6 s (N
sub-questions, each a fast vector+FTS+graph lookup; run concurrently where the served backend batches),
coverage #2 ≤ 5 s (small `max_tokens`), retrieval round 2 ≈ 2–4 s (pending only), final synthesis #3 ≈ 30–45 s
(the only full generation). Hard rule: only call #3 uses the full answer `max_tokens`; #1/#2 are capped small.

**Rationale**: Substantive answers measured at ~30–46 s today are generation-dominated; the loop adds one full
generation's worth of overhead spread across cheap steps + two quick retrieval rounds, landing ~1.5× — within
the ~65 s median target. Concurrency on round-1 sub-question retrieval (the backend batches) keeps it from
scaling linearly with sub-question count.

**Alternatives**: *Full generation for the draft (call #2)* — rejected: doubles the expensive step and blows the
budget; the draft only needs to be good enough to judge coverage, so it can be short.

## R8 — Concurrency & cancellation

**Decision**: Reuse the existing `DEEP_SEARCH` task-local for per-request state; the loop holds its working set
(sub-questions, passages) in local variables — no globals, no shared mutable state. Check the existing
cancellation token at each round boundary (after expand, after round 1, after coverage, after round 2) and
abort promptly if set.

**Rationale**: Per-request task-local is already the concurrency-safe mechanism deep uses (FR-010); round
boundaries are the natural, cheap cancellation points (FR-014) without interrupting an in-flight generation.

**Alternatives**: *A global "current deep run" registry* — rejected: not concurrency-safe across simultaneous
officers; the task-local already solves this.

## Open tuning (resolved with defaults; revisit in eval)

- Exact sub-question count (default 4–6) and coverage `max_tokens` (default ~256) — tune against the gold set.
- SC numeric targets (≥90% coverage, ≤65 s median, ≥60% A/B preference) — confirm on first A/B run.
