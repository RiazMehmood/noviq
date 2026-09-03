# Feature Specification: Answer Feedback Loop (Beta)

**Feature Branch**: `018-answer-feedback-loop`
**Created**: 2026-06-28
**Status**: Draft (ship WITH the agentic beta; start after 016, alongside/after 017)
**Input**: Beta strategy discussion 2026-06-28 (Riaz) + architect brief
`prompt_for_improvement.txt` (Evaluation Framework / Test Dataset sections). Related:
[[project_agentic-beta-roadmap]], ADR-0008 (citation integrity), 017 (the agent under test).

## Overview

The plan is to launch a **beta with the agentic approach**, gather **real-user feedback in
chat**, and use it to prioritize improvements (retrieval/prompt fixes first; reranker/embedding
fine-tune next; generator fine-tune last) and to build the **golden evaluation + regression
sets** that gate the final (GA) launch.

The feedback is deliberately **narrow: two citation questions only** — citation **accuracy**
(are the shown citations correct?) and citation **completeness** (did the answer cite everything
it should have?). No 👍/👎, no reason chips, no free-text box. Citation quality is the highest-value
legal signal, and keeping the surface to two one-tap questions maximizes response rate. For that
feedback to be trainable/diagnosable later, we still persist the **full answer trace** beside
every rating. This feature adds lightweight in-product feedback collection, an SME review queue,
and the promotion of curated examples into golden/regression sets — all **offline, local
PostgreSQL, no phone-home telemetry**.

This is additive (a new `answer_feedback` table + a two-question rating UI + an admin review
surface). It introduces **no model training infrastructure** — that is a separate, later effort
informed by the clusters this feature surfaces.

## Clarifications

### Session 2026-07-03

- Q: Citation *accuracy* option labels (officer-facing display text)? → A: **All citations
  correct** / **Some citations correct** / **Citations wrong/missing** / **Not sure** (maps to
  stored enum `all` / `some` / `wrong` / `unsure`).
- Q: Citation *completeness* option labels (officer-facing display text)? → A: **Fully cited** /
  **Some citations missing** / **Not applicable** (maps to stored enum `yes` / `missing` / `na`).

### Still to resolve (before/at planning)

- Sampling rate for the SME review queue and who has the SME/reviewer role.
- Beta-exit thresholds (citation-accuracy rate, citation-completeness rate, TTFT) — set with the architect.
- Retention/anonymization policy for stored queries (sensitivity).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - User rates citation quality with two one-tap questions (Priority: P1)

After reading an answer, the officer answers **two single-select questions** shown under the
answer:

- **Citation accuracy** — one of: *All citations correct* / *Some citations correct* /
  *Citations wrong/missing* / *Not sure* (stored as `all` / `some` / `wrong` / `unsure`).
- **Citation completeness** — one of: *Fully cited* / *Some citations missing* / *Not applicable*
  (stored as `yes` / `missing` / `na`).

Each is a single tap (chips / radio group). No 👍/👎, no reason chips, no free-text. The response
is stored with the full trace of how that answer was produced.

**Why this priority**: This is the primary feedback signal and the whole point of the beta —
without it there is no data to improve on. Citation quality is the highest-value legal signal. MVP.

**Independent Test**: Submit ratings on several answers; confirm each persists with its
`message_id`, the two citation values, and a complete answer trace, and is retrievable for analysis.

**Acceptance Scenarios**:

1. **Given** an answer is shown, **When** the user taps a citation-accuracy option, **Then** it is
   stored linked to that exact message with timestamp and user.
2. **Given** an answer is shown, **When** the user taps a citation-completeness option, **Then** it
   is stored alongside the accuracy value for the same message; either question may be answered
   independently.
3. **Given** any stored rating, **When** an operator inspects it, **Then** the full answer trace
   (query, decomposition, retrieved chunk IDs+scores, prompts, agent decisions/iterations,
   answer, citations, latency, engine, agent-on/off) is available.

---

### User Story 2 - Implicit signals are captured automatically (Priority: P2)

Even when users don't rate, the system records behavior that reveals satisfaction: copying the
answer (positive), regenerating/rephrasing the same question (negative), abandoning, or asking
a follow-up.

**Why this priority**: Most users won't rate; implicit signal multiplies coverage. Secondary to
explicit rating.

**Independent Test**: Perform copy / regenerate / follow-up actions and confirm each is logged
against the relevant message and distinguishable from explicit ratings.

**Acceptance Scenarios**:

1. **Given** a user copies an answer, **When** the action occurs, **Then** a positive implicit
   signal is recorded for that message.
2. **Given** a user immediately rephrases the same question, **When** detected, **Then** a
   negative implicit signal is recorded.

---

### User Story 3 - SME review queue produces clean labels and golden/regression sets (Priority: P3)

A legal subject-matter expert (admin/reviewer role) opens a queue of flagged answers (sampled
+ all answers marked *wrong/missing* or *some missing*), re-labels them with the correct
answer/citation where needed, and promotes confirmed
examples into the **golden set** (good) or **regression set** (must-improve).

**Why this priority**: The only channel clean enough to build training/eval data; needed for GA
gating, but can follow once explicit/implicit collection exists.

**Independent Test**: Route a flagged answer through the queue; an SME labels it; confirm it
lands in the golden or regression set and is usable by the offline eval harness (017 SC gate).

**Acceptance Scenarios**:

1. **Given** flagged answers, **When** an SME reviews one, **Then** they can mark it
   correct/incorrect, attach the correct citation/answer, and assign a category.
2. **Given** an SME confirms a good answer, **When** they promote it, **Then** it enters the
   golden set; a confirmed-bad answer enters the regression set.
3. **Given** the sets exist, **When** the eval harness runs, **Then** it reports metrics over
   them and flags regressions.

---

### Edge Cases

- **Low rating response rate**: most turns unrated → rely on implicit signals + SME sampling;
  don't block on coverage.
- **Noisy/biased ratings**: the two questions ask only about citations, so unrelated gripes (speed,
  tone) don't pollute the accuracy/completeness signal; SME re-labeling backstops any noise.
- **Sensitive queries**: a stored query/answer contains confidential content → access-controlled
  storage, role-gated review, retention/anonymization policy.
- **Trace missing**: an answer predates trace logging or a step failed to log → the rating is
  still stored but flagged "trace incomplete" (not silently dropped).
- **Duplicate/changed ratings**: a user changes a citation answer (e.g. *all correct* → *some
  correct*) → keep the latest with history, not a duplicate.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The UI MUST present **two single-select questions** on every answer message and
  nothing else: (1) **Citation accuracy** — labels *All citations correct* / *Some citations
  correct* / *Citations wrong/missing* / *Not sure*; (2) **Citation completeness** — labels
  *Fully cited* / *Some citations missing* / *Not applicable*. Rendered as one-tap chips or radio
  groups (no 👍/👎, no reason chips, no free-text comment).
- **FR-002**: Each question MUST be answerable independently (either one alone is valid) and MUST
  be **single-select**; re-tapping a different option updates the answer.
- **FR-003**: The system MUST persist each response linked to the exact `chat_message` with user,
  timestamp, and the two values `citation_accuracy` (all/some/wrong/unsure) and
  `citation_completeness` (yes/missing/na) in the additive `answer_feedback` table.
- **FR-004**: The system MUST persist a **full answer trace** per answer: query, decomposition,
  retrieved chunk IDs + scores, prompt(s), agent tool-call decisions/iterations, final answer,
  citations, latency, engine, and agent-on/off — so feedback is diagnosable and trainable.
- **FR-005**: The system MAY, in a later version, add **per-citation** accuracy marking (was
  citation [n] correct?) — out of scope for v1, which stays at the two answer-level questions.
- **FR-006**: The system MUST capture **implicit signals**: copy-answer, regenerate/rephrase,
  abandonment, follow-up — distinguishable from explicit ratings.
- **FR-007**: The system MUST provide an **admin/SME review queue** (all answers marked
  *wrong/missing* or *some missing* + a sample of others) to re-label answers with correct
  answer/citation and a category.
- **FR-008**: The system MUST support **promoting** reviewed examples into a **golden set**
  (good) or **regression set** (must-improve) consumable by the offline eval harness.
- **FR-009**: The system MUST provide an **aggregate view** of the two citation metrics over time
  (accuracy distribution and completeness distribution) to prioritize retrieval vs grounding fixes.
- **FR-010**: The system MUST support an **A/B comparison** of feedback between agent-on and
  agent-off (and engine variants) sessions, reusing the existing flag pattern.
- **FR-011**: All feedback storage MUST be **local (PostgreSQL) and offline** — **no external
  telemetry / phone-home**.
- **FR-012**: Feedback and traces MUST honor **data isolation and audit** (role-gated review;
  per-user ownership consistent with existing auth) and a defined **retention/anonymization**
  policy for sensitive content.
- **FR-013**: All schema changes MUST be **additive migrations only** (Constitution principle).
- **FR-014**: The system MUST define and report **beta-exit criteria** thresholds (citation-accuracy
  rate = share of "all correct", citation-completeness rate = share of "fully cited", TTFT) used to
  decide GA readiness.

### Key Entities

- **Answer Feedback**: id, `message_id` (FK), user, `citation_accuracy` (all / some / wrong /
  unsure), `citation_completeness` (yes / missing / na), created_at, supersedes (for changed
  answers).
- **Answer Trace**: the persisted production record of how an answer was built (see FR-004) —
  linked 1:1 to a message.
- **Citation Accuracy**: single-select — all correct / some correct / wrong or missing / not sure.
- **Citation Completeness**: single-select — fully cited / some missing / not applicable.
- **Implicit Signal**: type (copy/regenerate/abandon/follow-up), message ref, timestamp.
- **SME Review / Correction**: reviewer, verdict, corrected answer/citation, category, set
  assignment.
- **Golden / Regression Set Membership**: which curated Q→A examples gate eval/regression.

## Success Criteria *(mandatory)*

- **SC-001**: ≥ a defined share of answers receive at least one of the **two citation answers**
  during beta (target set with architect); 100% of answers have a **persisted trace**.
- **SC-002**: An operator can see the **distribution of both citation metrics** (accuracy and
  completeness) and use it to prioritize retrieval vs grounding fixes.
- **SC-003**: The **SME queue** is functional and produces curated labels; golden and regression
  sets are populated and consumed by the eval harness.
- **SC-004**: An **A/B view** compares agent-on vs agent-off (and engine) feedback.
- **SC-005**: **Beta-exit thresholds** are defined, measurable, and reportable.
- **SC-006**: **Zero external network calls** for feedback collection/storage (offline verified).
- **SC-007**: Feedback access respects **role-based isolation and audit**; retention policy is
  enforced.

## Assumptions

- The agentic beta (017) is live (or plain-RAG beta) producing answers to rate; feedback can be
  collected for either engine/agent state.
- Existing auth, roles (admin/user), `chat_message`, and `query_log` are reused and extended
  additively.
- The eval harness (built with 017 / this feature) consumes the golden/regression sets.

## Dependencies & Risks

- **Pairs with 017 (and 016)**: most valuable once the agent is live, but collection can begin
  on plain RAG.
- **Low response rate (MED)**: mitigated by one-tap UX + implicit signals + SME sampling.
- **Noisy ratings (MED)**: mitigated by asking only about citations (not speed/tone) and SME
  re-labeling for any training/eval use.
- **Privacy/sensitivity (MED)**: legal queries may be confidential → isolation, role-gated
  review, retention/anonymization.
- **Scope creep into training infra (MED)**: explicitly out of scope — this feature *surfaces*
  what to improve; it does not train.

## Out of Scope (Non-Goals)

- Model fine-tuning / training pipelines / RLHF infrastructure (separate, later — informed by
  this feature's clusters; generator fine-tune is the LAST resort per the ROI ladder).
- External analytics / cloud telemetry of any kind.
- The agent itself (feature 017) and the inference engine (feature 016).
- Automated acting on feedback (auto-retraining) — humans prioritize fixes in beta.
