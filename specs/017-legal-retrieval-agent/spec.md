# Feature Specification: Constrained Legal Retrieval Agent

**Feature Branch**: `017-legal-retrieval-agent`
**Created**: 2026-06-28
**Status**: Draft (start after 016-vllm-serving lands)
**Input**: Architect brief `prompt_for_improvement.txt` (AGENTIC AI section) + design
discussion 2026-06-28. Builds on 016 (vLLM `TextGenerator` seam), 008 (query decomposition),
ADR-0013 (one-ask composition), ADR-0008 (citation integrity), Principles XII/XIII
(amendment chain + cross-reference graph).

## Overview

Today the server answers each question with a **single-shot** RAG pass: retrieve once →
generate once. Complex legal questions (multi-part, multi-hop, cross-referenced, amended)
need **iterative** retrieval: rewrite the query, check whether the evidence is sufficient,
expand or traverse to fill gaps, verify citations, and only then answer.

This feature adds a **constrained Legal Retrieval Agent** — an iterative retrieval loop that
is **explicitly NOT** an autonomous/AutoGPT agent and has **no internet or open-ended tool
access**. The LLM is the *controller* (it decides what to retrieve next and when it has
enough); the existing Rust + PostgreSQL retrieval stack is the *executor*. The agent is
bounded by hard iteration and latency caps and **defaults to the grounded refusal**
("cannot be determined from the available documents") rather than ever fabricating.

A second goal is a **progressive answer experience**: for decomposable questions the agent
plans sub-questions up front, then retrieves → verifies → emits each part as it is ready,
showing live status ("working on approval limits…"), so the user sees verified content fast
even when total work exceeds the single-shot time.

This is a **server-only** change layered on the 016 inference engine. Retrieval correctness,
citation discipline, and offline operation are unchanged invariants.

## Clarifications

*(to resolve in /sp.clarify before planning)*

- Exact `MAX_ITER` cap (proposed 2–3) and per-turn latency budget split (TTFT vs total).
- Which control steps use vLLM **guided decoding** vs **native Qwen3 tool-calling**.
- Whether the agent is default-on in beta or behind an `agent on/off` flag (recommended: flag, for A/B).
- Confidence-estimation method (retrieval-score / entity-coverage / LLM self-rating).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Iterative retrieval answers complex questions single-shot misses (Priority: P1)

A legal officer asks a multi-part or cross-referenced question (e.g. "Compare the approval
limits of DDWP and CDWP and note any amendments"). The agent decomposes it, retrieves per
part, detects a missing piece, re-retrieves to fill it, verifies citations, and returns a
complete, grounded, cited answer that a single retrieval pass would have left incomplete.

**Why this priority**: This is the feature's core value — measurably better completeness and
correctness on hard questions. It is the MVP.

**Independent Test**: On a curated multi-hop / multi-part question set, the agent's answers
score higher on completeness and citation recall than the single-shot baseline, with no
increase in hallucinated or missing citations, within the latency budget.

**Acceptance Scenarios**:

1. **Given** a multi-part question whose evidence spans several documents, **When** the agent
   runs, **Then** every part is answered with at least one valid citation and no required part
   is silently dropped.
2. **Given** a question needing a cross-reference/amendment hop, **When** the first retrieval
   is insufficient, **Then** the agent performs a bounded additional retrieval and the final
   answer reflects the **current** (post-amendment) law.
3. **Given** the same question set, **When** compared to single-shot RAG, **Then** completeness
   and citation recall improve with **no regression** in grounding/citation precision.

---

### User Story 2 - Verified answers stream progressively with live status (Priority: P2)

For a decomposable question, the user sees the answer build up part-by-part — each part
appears only after it is grounded and citation-verified — with a live indicator of what the
agent is currently working on, so perceived latency is low.

**Why this priority**: High UX value, but secondary to producing the correct answer at all.

**Independent Test**: For a multi-part question, confirm the first verified section is visible
well before the full answer completes, status updates reflect the current stage, and no shown
section is later contradicted or retracted.

**Acceptance Scenarios**:

1. **Given** a decomposable question, **When** the agent runs, **Then** the first verified
   section surfaces well before total completion and a status message names the current step.
2. **Given** parts that depend on a shared amendment, **When** sections stream, **Then** no
   emitted section is contradicted by a later one (plan-globally / emit-locally holds).
3. **Given** a non-decomposable multi-hop question, **When** it runs, **Then** it completes
   fully before emitting (no provisional, retractable content).

---

### User Story 3 - The agent is bounded, safe, and never fabricates (Priority: P3)

Under any input the agent stays within its iteration and time budget, and when evidence is
insufficient it returns the explicit refusal rather than guessing.

**Why this priority**: Safety/bounds are essential for a government system but can be validated
once the loop (US1) exists.

**Independent Test**: Adversarial / no-evidence / ambiguous questions never exceed `MAX_ITER`
or the budget, never fabricate, and produce the grounded refusal where appropriate.

**Acceptance Scenarios**:

1. **Given** a question with no supporting evidence, **When** the agent exhausts its bounded
   retrieval, **Then** it returns "cannot be determined from the available documents", not a
   fabricated answer.
2. **Given** a question that keeps retrieving the same chunks, **When** no new evidence is
   found, **Then** the agent stops on no-progress rather than looping to the budget ceiling.
3. **Given** any question, **When** measured, **Then** iterations ≤ `MAX_ITER` and the turn
   never hangs (graceful stop on time budget).

---

### Edge Cases

- **Sequential-call latency**: within one turn the control calls are sequential (sufficiency
  needs the rewrite) — they cannot batch together for that user; iterations must be capped.
- **No-progress loop**: a new iteration retrieves a subset of the previous chunks → must stop.
- **Amendment ordering**: a later part retrieves an amendment affecting an earlier emitted part
  → prevented by resolving amendments in the planning phase before any emit.
- **Guided-decoding failure**: a control step returns malformed structure → bounded retry then
  safe fallback (treat as "insufficient"/refuse), never crash.
- **Engine unavailable mid-loop**: vLLM down/timeout (016 FR-011) → graceful message / fallback.
- **Over-decomposition**: a simple lookup must NOT pay the full agent cost — cheap questions
  take a fast single-shot path.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST run a bounded **iterative retrieval loop** in which the LLM
  decides the next action and the existing Rust/PostgreSQL retrieval executes it.
- **FR-002**: The agent's controllable actions MUST include: query rewrite, metadata/filter
  routing, retriever-mode selection (lexical + vector + metadata + RRF), neighbour/section
  expansion, and graph traversal (cross-reference / amendment / authority).
- **FR-003**: Control decisions (sufficiency, routing, verification, rewrite) MUST be produced
  as **structured outputs** via vLLM guided decoding (or native tool-calling), validated before
  use; malformed output triggers bounded retry then a safe default.
- **FR-004**: The agent MUST perform an **evidence-sufficiency check** each iteration and
  **stop when sufficient**; sufficiency output MUST identify missing pieces to drive the next
  retrieval.
- **FR-005**: The loop MUST enforce a hard **maximum iteration cap** and a **per-turn time
  budget**; exceeding either MUST stop the loop gracefully.
- **FR-006**: On stop-without-sufficiency the system MUST return the explicit grounded refusal,
  **never a fabricated answer** (preserves Principle XI / citation discipline).
- **FR-007**: The system MUST detect **no-progress** (a new retrieval adds no new relevant
  evidence) and stop rather than spin.
- **FR-008**: For decomposable questions the agent MUST **plan sub-questions and resolve
  amendments/cross-references up front** (plan globally), then retrieve→verify→emit each part
  (emit locally), so emitted sections cannot contradict later ones.
- **FR-009**: A section MUST be **verified (grounding + citation integrity) before it is
  emitted**; never emit-then-check. Non-decomposable multi-hop questions complete before emit.
- **FR-010**: The system MUST stream **live status** of the current step to the existing
  `chat://` event channel without frontend protocol changes.
- **FR-011**: Every emitted statement MUST map to retrieved evidence and pass the existing
  deterministic **citation-integrity** check (ADR-0008); fabricated citations MUST be rejected.
- **FR-012**: The agent MUST be selectable via a config flag (**agent on/off**) so beta can
  **A/B** agentic vs single-shot RAG and instantly roll back.
- **FR-013**: Simple/lookup questions MUST take a **fast path** (single-shot) and not incur the
  full iterative cost.
- **FR-014**: The agent MUST reuse the **016 `TextGenerator` seam** for all model calls; it
  MUST NOT introduce a second inference engine or any Python service in the hot path.
- **FR-015**: The system MUST remain **fully offline** — no external calls; all tools are local
  Rust/PostgreSQL/graph operations.
- **FR-016**: Answer quality MUST be gated by a **two-tier eval**: inline per-answer guardrail
  (citation integrity + groundedness) and an **offline regression gate** on a golden set before
  the agent is enabled by default.

### Key Entities

- **Agent Loop State**: current iteration, elapsed time, accumulated evidence, retrieved-chunk
  history (for no-progress detection), sub-question plan, per-part status.
- **Tool-Call Decision**: a validated structured action (rewrite / route / expand / traverse /
  sufficiency / verify) the LLM emits and Rust dispatches.
- **Retrieval Tool**: an executable Rust/SQL operation — lexical, vector, metadata, RRF fusion,
  neighbour/section expansion, graph traversal.
- **Sufficiency Verdict**: `{ sufficient, missing[] , confidence }` driving stop vs next query.
- **Sub-question Plan**: the up-front decomposition + amendment resolution for progressive emit.
- **Emitted Section**: a verified, citation-checked answer fragment streamed to the user.

## Success Criteria *(mandatory)*

- **SC-001**: On a multi-hop / multi-part legal question set, **completeness and citation
  recall improve** vs single-shot RAG, with **no regression** in citation precision or
  groundedness.
- **SC-002**: **Hallucination / fabricated-citation rate does not increase** vs the baseline.
- **SC-003**: **Time-to-first-verified-section < ~3 s** and full answer **< ~10–15 s** at the
  target concurrency (reconciled with 016 budgets).
- **SC-004**: **100% of turns stay within `MAX_ITER` and the time budget** (zero unbounded
  loops, zero hangs).
- **SC-005**: On no-evidence/ambiguous questions the agent returns the **grounded refusal**,
  not a fabrication, in 100% of cases tested.
- **SC-006**: No emitted section is ever **retracted or contradicted** by a later section.
- **SC-007**: Fully **offline** — zero external network calls during an agent turn.
- **SC-008**: The **agent on/off A/B** flag works and the offline regression gate passes before
  default-on.

## Assumptions

- Feature 016 (vLLM engine + `TextGenerator` seam, guided decoding / Qwen3 tool-calling, prefix
  caching) is complete and in production.
- Existing retrieval (lexical+vector+metadata+RRF, neighbour/hierarchical chunking), the
  cross-reference/amendment graph, and citation-integrity logic are correct and reused as tools.
- A golden evaluation set (built with / alongside feature 018) exists to gate cutover.

## Dependencies & Risks

- **Depends on 016 (HARD)**: no agent without concurrent, structured, fast generation.
- **Sequential-call latency (HIGH)**: per-turn control calls don't batch; mitigated by
  `MAX_ITER` cap, tiny guided control calls, prefix caching, fast path for simple questions.
- **Structured-output reliability (MED)**: guided decoding must be robust; bounded retry + safe
  default.
- **Eval dependency (MED)**: without the golden set + inline guardrail, agentic gains are
  unmeasurable and risky — build eval first.
- **Complexity (MED)**: the loop + stopping + progressive-emit policy is the real engineering
  cost (not the plumbing).

## Out of Scope (Non-Goals)

- Autonomous / AutoGPT-style agents, internet tools, or open-ended tool registries.
- Any new inference engine or a Python agent service in the hot path.
- Model fine-tuning (feedback-driven improvement is feature 018 / later).
- The feedback-collection system itself (feature 018).
- Multi-GPU / distributed inference (016 FR-015 keeps it possible later).
