# ADR-0021: Bounded Agentic Multi-Pass Deep Retrieval

> **Scope**: Document decision clusters, not individual technology choices. Group related decisions that work together.

- **Status:** Accepted
- **Date:** 2026-07-13
- **Feature:** 025-agentic-deep-search
- **Context:** An audit (2026-07-13) proved the existing one-pass "Deep" toggle cannot show the model more law
  than a normal search. Both fill the same ~36-passage / ~5,400-token context window; the graph cross-reference
  and amendment arms already run for normal search in production (`ECHO_GRAPH_EXPAND=1`); and Deep's intended
  "more room for law" lever (`ECHO_DEEP_BUDGET_MULT`) is structurally dead code (`min(X·1.5, X) = X`). Deep's
  only real edge was a wider candidate pool + one recall arm + a breadth prompt — a measured but modest +21%.
  The binding constraint is a **single saturated context window**. Broad, multi-part legal questions ("grounds,
  penalties, procedure, and appeal under the E&D Rules"; "list all Schedule-I departments") therefore stay
  incomplete no matter how many candidates are gathered. We must make Deep genuinely deeper without a hardware
  or model-window change, and without regressing the normal path or introducing hallucination.

<!-- Significance checklist: (1) Impact — introduces a multi-pass orchestration pattern + shared-retrieval
     extraction, long-term structural. (2) Alternatives — bigger window/GPU, single-pass+wider pool, unbounded
     agent, map-reduce. (3) Scope — cross-cutting across retrieval, generation, tracing, flags. All true. -->

## Decision

Replace the one-pass Deep path with a **bounded agentic multi-pass loop** that answers a query across several
fresh context windows instead of one saturated window. The cluster:

- **Orchestration pattern**: exactly **≤ 3 LLM calls** and **≤ 2 retrieval rounds** per Deep turn —
  (1) decompose + LLM **facet-expansion** into 4–6 sub-questions (the original query is always retained as a
  retrieval leg); (2) retrieval round 1 per sub-question; (3) a short **draft + coverage-check** call; targeted
  retrieval round 2 for **pending** sub-questions only; (3rd call) **grounded final synthesis** over the deduped,
  budget-packed union. No self-extension, no unbounded iteration.
- **Coverage-check as the routing gate (the one new component)**: a short, structured call emitting only
  per-sub-question `answered|pending` (no answer prose, so it cannot fabricate content). It **biases to
  re-fetch** on any parse failure or uncertainty. This is what turns "one wider pass" into "a second targeted
  pass where it's needed."
- **Reuse boundary**: per-sub-question retrieval reuses the existing `query_router` hybrid path (vector + FTS +
  RRF + graph + summary arms) via an **extracted shared `retrieve_for_query`** — NOT `one_ask::research()`,
  which is drafting-coupled. The anti-hallucination gate reuses `one_ask`'s low-level primitives
  (`quote_verifies`, `foreign_number_in_prose`); cross-round packing reuses `summary_builder::pack_windows`
  (pending-slot passages first).
- **Grounding gate (hard)**: every citation/figure in the final answer must trace to a retrieved passage;
  unverifiable content is dropped/flagged, and if nothing verifies the turn returns the grounded-fallback rather
  than ungrounded prose (Constitution XI).
- **Stateless + concurrency-safe**: no new DB schema; per-request `DEEP_SEARCH` task-local; cancellation checked
  at each round boundary.
- **Flag-gated hard-cutover with A/B**: gated behind `ECHO_AGENTIC_DEEP` (default off). `deep=true` + flag-on →
  agentic loop; `deep=true` + flag-off → legacy one-pass deep. The legacy path is retired only after the A/B in
  `research/025-agentic-deep-search/` clears the completeness / traceability / latency gates.

## Consequences

### Positive

- **Breaks the single-window ceiling with no infra change** — multiple fresh windows read far more law across a
  turn than one pass can, on the existing 8192 window and current GPU.
- **Completeness on multi-part questions** — the coverage-check + targeted round 2 directly fills the parts a
  single pass leaves thin (target ≥ 90% sub-part coverage).
- **Stronger grounding than before** — the explicit verify gate (motivated by the live "Department Zoonology"
  fabrication) makes refuse-or-flag the default; citation discipline is enforced, not hoped for.
- **Bounded and predictable** — the ≤3-call/≤2-round cap makes latency and cost knowable (~65 s median target),
  avoiding runaway-agent behavior.
- **Mostly assembly, small blast radius** — reuses decomposition, the hybrid+graph+summary retrieval core, and
  verify primitives; the only shared-code change is a behavior-preserving retrieval extraction. Normal search is
  untouched.
- **Reversible** — flag-gated A/B against the legacy path before cutover.

### Negative

- **Latency** — Deep rises to ~65 s median / ≤ ~90 s p95, exceeding the ≤10 s Tier-3/4 dev-constraint budget.
  Accepted because Deep is opt-in and explicitly labeled "thorough, slower"; normal queries keep the budget.
- **Coverage-check is a new judgment surface** — it can mis-classify a part as covered (→ omission) or pending
  (→ a wasted round). Mitigated by bias-to-refetch and the one-round bound, but it is genuine new prompt-tuning
  surface with its own eval.
- **Three sequential model calls per turn** raise total token/compute per Deep query (kept in check by making
  only call #3 a full generation).
- **Shared-retrieval extraction risk** — refactoring `route_scoped_with_override` could subtly change the
  normal path if not behavior-preserving; guarded by an FR-008/INV-7 regression check.
- **Temporary dual code paths** — legacy + agentic deep coexist until the A/B completes (extra maintenance during
  the window).

## Alternatives Considered

- **Enlarge the context window / GPU (raise vLLM `max_model_len` 8192→16384)** — would let a single pass see more
  law and grows the budget for both modes. Rejected as the *primary* fix: costs VRAM on the A40, is an infra
  lever not a per-request capability, and still caps at one window. Remains a complementary future option (would
  further help the agentic loop too).
- **Single-pass + wider candidate pool / fix the dead budget-mult knob** — the status quo, minus the bug.
  Rejected: the audit shows more candidates cannot help when only ~36 reach the model in one window; reranking a
  wider pool into the same slots can even *drop* passages normal kept.
- **Unbounded / self-directed agent (ReAct-style, iterate until "done")** — maximum flexibility. Rejected:
  unpredictable latency and cost, hard to bound and evaluate; conflicts with the officer-UX need for a knowable
  wait. The bounded ≤3/≤2 loop captures most of the benefit with none of the runaway risk.
- **Map-reduce over the whole corpus (summarize-then-answer)** — good for whole-document summaries (already used
  there). Rejected for interactive Q&A: much higher latency and weaker precision on targeted multi-part legal
  questions than targeted decomposition + retrieval.
- **Reuse `one_ask::research()` / `assemble_and_verify()` wholesale** (the original seed) — rejected: both are
  drafting-coupled (`OneAskIntent`, section skeletons, `ProvisionCard`, per-section citation caps); bending them
  to Q&A is more code and risk than extracting the shared retrieval fn and reusing only the verify primitives.

## Addendum (2026-07-13, implementation)

**Reuse-boundary refinement (T004).** The plan/decision said "**extract** a shared `retrieve_for_query` from
`route_scoped_with_override` and have the normal path call it." In implementation that function proved to be a
single **~1,400-line** hot path with retrieval and generation deeply interleaved — a behavior-preserving
extraction is a large, high-risk refactor. Architect decision: **compose, don't extract** — add a new
`retrieve_for_query` that reuses the already-standalone primitives (`get_context_chunks`, `fts5_retrieve`,
`union_merge`/`rrf_merge`, and the summary-recall + graph-expand arms), leaving `route_scoped_with_override`
untouched. This makes the normal path byte-for-byte unchanged (zero regression risk) at the cost of the
arm-composition sequence existing in two places (the two must be kept in sync). The decision's substance —
one shared set of retrieval primitives feeding both normal and deep — is preserved.

## References

- Feature Spec: specs/025-agentic-deep-search/spec.md
- Implementation Plan: specs/025-agentic-deep-search/plan.md (+ research.md, data-model.md, contracts/)
- Related ADRs: ADR-0009 (query decomposition gated to capacity-limited generators — extended here with LLM
  facet expansion), ADR-0013 (one-ask summary composition — source of the reused verify primitives), ADR-0019
  (structured counting & verbatim synthesis retrieval), ADR-0020 (graph-augmented retrieval on Postgres — the
  graph arm reused per sub-question), ADR-0018 (vLLM inference engine — the served model the calls run on).
- Evaluator Evidence: history/prompts/025-agentic-deep-search/0002-agentic-deep-search-plan.plan.prompt.md
  (Constitution Check PASS with one justified latency deviation); A/B gates to be recorded under
  research/025-agentic-deep-search/ after first run.
