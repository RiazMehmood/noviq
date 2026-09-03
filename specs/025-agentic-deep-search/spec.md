# Feature Specification: Agentic Deep Search

**Feature Branch**: `025-agentic-deep-search`
**Created**: 2026-07-13
**Status**: Draft
**Input**: User description: "Replace the one-pass Deep toggle with a bounded 3-LLM-call / 2-retrieval-round agentic loop (decompose+expand → retrieve → draft+coverage-check → re-retrieve pending → synthesize) that beats the single-window ceiling by using multiple fresh windows."

## Context & Problem *(why this exists)*

An audit (2026-07-13) established that the current "Deep" toggle does **not** let the model see more law
than a normal search. Both use the same ~36-passage / ~5,400-token context window, and the graph
cross-reference + amendment expansion already runs for normal searches in production. Deep's only real edge
today is a wider candidate pool, one extra recall arm, and a "be comprehensive" instruction — a measured but
modest +21% answer. The binding constraint is a **single saturated context window**: no matter how many
candidates are gathered, only ~36 passages reach the model in one pass.

This feature removes that ceiling **without new hardware or a larger model window** by making Deep a
**bounded multi-pass loop**. Instead of one over-full window, the officer's question is answered across several
fresh windows: the question is broken into its parts, each part is researched, the draft answer is checked for
which parts are still unanswered, the missing parts get one more targeted research round, and a final grounded
answer is composed from everything gathered. The loop is strictly bounded (a fixed, small number of reasoning
steps and research rounds) so it can never run away, and every fact in the final answer is verified against a
retrieved passage so the extra breadth never comes at the cost of made-up content.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Complete answers to multi-part questions (Priority: P1)

An officer asks a broad, multi-part legal question with Deep turned on — e.g. *"What are the grounds,
penalties, procedure, and right of appeal under the E&D Rules?"* Today the answer tends to cover the two or
three most obvious parts and thin out on the rest, because everything competes for one window. With this
feature, the question is split into its distinct parts, each is researched, and any part the first draft left
thin is researched again before the final answer — so the officer receives a response that addresses **every**
part of what they asked.

**Why this priority**: This is the entire point of Deep — completeness on questions too broad for a single
window. Without it, the toggle delivers little beyond normal search.

**Independent Test**: Pose a question with N clearly separable parts. A rater confirms the Deep answer
substantively addresses each of the N parts (not "several"/hand-waving), and addresses more of them than the
current one-pass Deep answer to the same question.

**Acceptance Scenarios**:

1. **Given** a 4-part legal question and Deep ON, **When** the officer submits it, **Then** the answer contains
   a distinct, grounded treatment of all 4 parts.
2. **Given** the same question with Deep OFF, **When** the officer submits it, **Then** behavior and latency
   match the pre-feature normal search (no change).
3. **Given** a question where the first research round already covers every part, **When** the coverage check
   finds nothing pending, **Then** the system composes the final answer without a wasted second round.

---

### User Story 2 - Trustworthy, grounded answers (Priority: P1)

Because officers act on these answers, the added breadth must not introduce invented content. Every citation,
provision label, and figure in the final Deep answer must trace back to a passage that was actually retrieved;
anything that cannot be traced is dropped or flagged rather than presented as fact.

**Why this priority**: A more complete answer that is partly fabricated is worse than a short correct one. This
is a hard gate on the feature, co-equal with completeness. (Directly motivated by the live "Department
Zoonology" hallucination this design must prevent.)

**Independent Test**: Run the Deep answer through a grounding audit — every cited fact must map to a retrieved
passage. Include a query designed to tempt fabrication (sparse/garbled source rows); confirm the answer omits
the unsupported claim instead of inventing one.

**Acceptance Scenarios**:

1. **Given** a Deep answer with citations, **When** each citation is audited, **Then** 100% trace to a
   retrieved passage.
2. **Given** a sub-question for which nothing relevant was retrieved, **When** the final answer is composed,
   **Then** the system states the gap plainly and does not fabricate an entry to fill it.

---

### User Story 3 - Predictable, bounded wait (Priority: P2)

The officer accepts that Deep is slower, but it must be **predictably** slower, not open-ended. The loop runs a
fixed, small number of research rounds and reasoning steps regardless of the question, so the wait stays within
a known band and the system never spins.

**Why this priority**: An unbounded "agent" that occasionally takes minutes would be unusable in practice and
erode trust in the toggle. Boundedness is what makes the multi-pass approach shippable.

**Independent Test**: Measure end-to-end latency across a representative query set; confirm every Deep turn uses
no more than the fixed cap of research rounds/reasoning steps and completes within the target band.

**Acceptance Scenarios**:

1. **Given** any question with Deep ON, **When** it runs, **Then** it performs at most the fixed number of
   research rounds and reasoning steps.
2. **Given** a representative query set, **When** latencies are measured, **Then** the median stays within the
   target band and no turn runs unbounded.

---

### User Story 4 - Officer can see Deep is working (Priority: P3)

During the longer wait and in the debug/trace view, the officer/administrator can see what Deep is doing — the
parts the question was split into, which parts the first pass answered, and which triggered a second research
round — so the extra time is legible and the feature is measurable in A/B.

**Why this priority**: Observability turns a slow black box into a trustworthy tool and is required to evaluate
whether Deep earns its cost, but it is not required for the core answer to be correct.

**Independent Test**: Submit a Deep query; confirm the trace shows the sub-questions, per-part coverage, and the
pending items that caused the second round.

**Acceptance Scenarios**:

1. **Given** a Deep query, **When** the administrator opens the query trace, **Then** it lists the
   sub-questions, each part's answered/pending status, and which pending parts triggered round two.

---

### Edge Cases

- **Single-facet or trivial question**: decomposition yields one part (or the question is a greeting / intro /
  "@document not found"). The loop degrades gracefully to a near single-pass with no wasted rounds.
- **Bad or empty decomposition**: the original question is always kept as a retrieval target, so a poor split
  cannot starve retrieval.
- **First round already complete**: the coverage check finds nothing pending → skip the second research round.
- **Coverage check uncertain**: when it cannot confidently decide a part is covered, it errs toward
  re-retrieving that part (cheap, since only one extra round exists) rather than risk omission.
- **Nothing retrievable for a part**: the part is reported unanswerable and stated as a gap; no fabrication.
- **Gathered passages exceed the final window**: passages are deduplicated across rounds and packed to budget,
  prioritizing the passages that answer the previously-pending parts.
- **Existing scoping still honored**: `@document` scoping, confidential masking, follow-up carry-over, and
  no-answer/greeting fast-paths behave as they do for normal search.
- **Cancellation**: if the officer cancels mid-loop, the turn stops promptly at the next round boundary.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When Deep is ON, the system MUST expand the officer's question into multiple sub-questions
  (its distinct parts/facets) and MUST always retain the original question as one retrieval target.
- **FR-002**: The system MUST perform an initial research round covering every sub-question and produce a first
  draft answer from the passages retrieved.
- **FR-003**: After the draft, the system MUST assess, per sub-question, whether it is adequately answered or
  still pending ("coverage check").
- **FR-004**: For sub-questions judged pending, the system MUST perform exactly one additional targeted research
  round before composing the final answer; sub-questions judged covered MUST NOT trigger extra retrieval.
- **FR-005**: The loop MUST be strictly bounded per Deep turn: at most three model reasoning steps and at most
  two research rounds, with no unbounded or self-extending iteration.
- **FR-006**: The final answer MUST be grounded — every citation, provision label, and quoted figure MUST trace
  to a retrieved passage; unverifiable content MUST be dropped or flagged rather than presented as fact.
- **FR-007**: When the coverage check cannot confidently determine that a sub-question is covered, the system
  MUST bias toward re-retrieval rather than omission.
- **FR-008**: The Deep toggle MUST invoke this loop, fully replacing the prior one-pass Deep path; when Deep is
  OFF, search behavior and latency MUST be unchanged from the pre-feature normal search.
- **FR-009**: The intermediate reasoning steps (decomposition/expansion and coverage check) MUST be kept
  lightweight so that only the final synthesis is a full-length answer, keeping total latency within target.
- **FR-010**: A Deep turn MUST be per-request and concurrency-safe — no state leaks between simultaneous users
  or turns.
- **FR-011**: The system MUST expose, in the query trace/debug view, the sub-questions, each sub-question's
  coverage status, and which pending items triggered the second research round.
- **FR-012**: For single-facet or trivial questions, the loop MUST degrade gracefully (no wasted rounds) and
  MUST honor existing greeting/intro and no-answer fast-paths.
- **FR-013**: The final assembled passage set MUST be deduplicated across research rounds and packed to the
  answer window's budget, prioritizing passages that resolve previously-pending sub-questions.
- **FR-014**: The system MUST support prompt cancellation, stopping the loop at the next round boundary.

### Key Entities *(include if feature involves data)*

- **Sub-question (facet)**: one distinct part of the officer's question produced by decomposition/expansion —
  a short question text, optionally with structured slots (instrument, provision, subject).
- **Coverage report ("the metrics")**: for each sub-question, an answered/pending judgment plus a pointer to the
  passage(s) that support the "answered" verdict. Drives the targeted second research round.
- **Retrieved passage set**: the union of round-one and round-two passages, deduplicated and budget-packed, that
  the final answer is composed from and verified against.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a gold set of multi-part legal questions, Deep answers substantively address **≥ 90%** of the
  distinct sub-parts a human rater identifies, and address **more** sub-parts on average than the current
  one-pass Deep baseline.
- **SC-002**: **100%** of citations/quoted figures in Deep answers trace to a retrieved passage (zero
  fabricated entries) across the evaluation set.
- **SC-003**: Deep turns complete within a predictable band — target **≤ ~65 s median** and a bounded tail
  (**≤ ~90 s p95**) — and **every** turn uses no more than the fixed cap of research rounds and reasoning steps.
- **SC-004**: On the "enumerate the items in a large table/schedule" class of question (e.g. the RoB Schedule-I
  departments), Deep names **materially more** correct items than the current one-pass Deep answer.
- **SC-005**: With Deep OFF, normal-search answer content and latency are unchanged from the pre-feature
  baseline (within measurement noise) — no regression.
- **SC-006**: In an A/B against the current one-pass Deep, raters prefer the new Deep answer for completeness on
  **≥ 60%** of multi-part questions, with no increase in ungrounded claims.

## Assumptions

- Reuses the corpus, graph overlay, and per-provision summary index already built by earlier features; no
  re-ingestion is required for this feature.
- The model context window stays at its current size — this feature explicitly does **not** depend on a larger
  window or additional GPU capacity (that remains a separate, optional infrastructure lever).
- "Sub-question coverage" is judged by a lightweight model check and validated against a human-rated gold set;
  the gold set is assembled from existing evaluation questions plus the multi-part questions in current logs.
- During the longer Deep wait, progress is surfaced through the existing "thinking" progress channel (parts
  found → researching → checking coverage → fetching more on pending → composing); no new streaming surface is
  required.
- Default breadth is 4–6 sub-questions; the exact number adapts to the question and is bounded so intermediate
  steps stay cheap.

## Dependencies

- Existing query decomposition capability (extended with model-driven facet expansion).
- Existing research/retrieval capability, including the graph cross-reference and summary-recall arms.
- Existing answer-grounding/verification capability that checks generated prose against retrieved passages.
- Existing Deep-toggle plumbing (per-request flag) and query-trace/debug view.

## Out of Scope

- Enlarging the model context window or changing serving hardware.
- Any change to normal (Deep OFF) search behavior.
- Unbounded or self-directed agent behavior (more than the fixed rounds/steps).
- New persistent storage or schema changes (the loop is stateless per turn).
- Cross-document "sum over all documents" aggregation (a separate future feature).
