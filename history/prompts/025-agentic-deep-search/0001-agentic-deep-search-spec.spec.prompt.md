---
id: 0001
title: agentic-deep-search-spec
stage: spec
date: 2026-07-13
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: /sp.specify
labels: [deep-search, agentic-rag, retrieval, spec]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/025-agentic-deep-search/spec.md
  - specs/025-agentic-deep-search/checklists/requirements.md
tests:
  - none (specification phase — no code)
---

## Prompt

Design/spec followed a live audit of the existing Deep toggle. Architect's decisions (captured via
AskUserQuestion before specifying): REPLACE the current one-pass deep with the agentic loop; use LLM facet
expansion for decomposition; write a spec next.

Feature description (verbatim seed):

> Agentic deep search — replace the current one-pass "Deep" toggle with a bounded 3-LLM-call /
> 2-retrieval-round agentic loop that beats the saturated 8192-token single-window ceiling by using multiple
> fresh windows instead of one.
>
> FLOW (bounded): (1) LLM call #1 — decompose + LLM facet expansion into 4–6 sub-questions, always keep the
> original query as a retrieval leg; (2) retrieval round 1 over each sub-question with graph + summary arms,
> RRF-merge; (3) LLM call #2 — draft answer + coverage check (answered vs pending), bias to re-fetch when
> unsure; (4) retrieval round 2 — targeted recall for pending only; (5) LLM call #3 — final grounded synthesis
> over round-1 ∪ round-2 chunks (dedup + budget-aware packing).
>
> REUSE: decompose::detect_and_split (F008, add LLM expansion); one_ask/research.rs (research, rrf_merge,
> relevant_hits); one_ask/verify.rs (assemble_and_verify, quote_verifies, foreign-number flag) as the
> anti-hallucination guard; existing graph_expand + summary_recall arms; DEEP_SEARCH task-local plumbing.
>
> SCOPE: replaces one-pass deep; normal search unchanged. NON-GOALS: no bigger window/GPU; no unbounded agent.
> CONSTRAINTS: calls #1/#2 short/structured (only #3 full generation), total ~55–65s; grounded via verify.rs;
> A/B measurable (completeness + coverage + latency).

## Response snapshot

Created branch `025-agentic-deep-search` (numbered 025 — 024 was already burned by the graph-guided
summarizer commits). Wrote `specs/025-agentic-deep-search/spec.md` from the spec template:

- **Context & Problem**: the single saturated window is the ceiling; graph/amendment expansion already runs
  for normal search in prod (`ECHO_GRAPH_EXPAND=1`), so one-pass deep's edge is small. Fix = bounded multi-pass
  (multiple fresh windows), no new hardware.
- **4 user stories** (prioritized): P1 complete multi-part answers; P1 trustworthy/grounded (motivated by the
  live "Department Zoonology" hallucination); P2 predictable bounded wait; P3 observable "deep is working".
- **14 functional requirements** (FR-001…FR-014): decompose+keep-original, initial round + draft, coverage
  check, exactly-one targeted second round, strict bound (≤3 reasoning steps / ≤2 rounds), grounding gate,
  bias-to-refetch, replace one-pass deep with normal unchanged, cheap intermediate steps, per-request safety,
  trace observability, graceful degrade, dedup+budget packing, cancellation.
- **6 success criteria**: ≥90% sub-part coverage; 100% citations traceable (zero fabrication); ≤~65s median /
  ≤~90s p95 and always bounded; materially more correct items on enumerate-a-table questions; no normal-search
  regression; ≥60% A/B completeness preference with no ungrounded increase.
- Assumptions, Dependencies (reuse map at behavior level), Out of Scope sections filled.

Quality checklist written and validated: **PASS on all items, zero [NEEDS CLARIFICATION]** (scope decisions
pre-made; remaining gaps resolved via documented Assumptions defaults).

## Outcome

- ✅ Impact: Spec + quality checklist ready for planning; feature branch 025 created and checked out.
- 🧪 Tests: none (spec phase).
- 📁 Files: specs/025-agentic-deep-search/spec.md, specs/025-agentic-deep-search/checklists/requirements.md.
- 🔁 Next prompts: `/sp.plan` (map FRs to concrete reuse — decompose LLM-expansion, one_ask research/verify
  wiring, coverage-check contract, packing precedence, trace fields); optional `/sp.clarify` to tune SC numeric
  targets against the assembled gold set. ADR likely owed (agentic multi-pass over single-pass RAG).
- 🧠 Reflection: The decisive insight was that this is assembly, not greenfield — decompose (F008), one_ask
  research/verify, and graph/summary arms already exist; only the coverage-check ("metrics") is genuinely new.
- 🧠 Reflection: Grounding gate (verify.rs) is elevated to a co-P1 story specifically because the immediately
  preceding work exposed a real hallucination ("Department Zoonology"); the spec must prevent that class.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (spec phase). Anticipated risks documented as edge cases: coverage-check
  mis-judgment (mitigated by bias-to-refetch), decomposition error propagation (mitigated by original-query
  leg), cross-round context overflow (mitigated by dedup + budget packing).
- Graders run and results (PASS/FAIL): spec quality checklist — PASS (all items).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): during /sp.plan, prototype the coverage-check as a small structured
  call and measure its latency + judgment accuracy on 5–10 multi-part questions before wiring the full loop.
