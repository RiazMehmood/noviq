# Feature Specification: Query Decomposition for Multi-Fact Questions

**Feature Branch**: `008-query-decomposition`
**Created**: 2026-06-07
**Status**: Draft
**Input**: User description: "Query decomposition for multi-fact questions in the legal RAG chat pipeline."

## User Scenarios & Testing *(mandatory)*

The "user" is a government officer asking the legal assistant questions about a
constitutional / statutory corpus. Today, single-fact questions ("What does Article 9
guarantee?") get accurate answers, but questions that ask for **two or more facts at
once** come back scrambled — wrong article numbers, self-contradiction, or one half
of the answer pulled from the wrong provision. The officer cannot trust a multi-part
answer. This feature makes multi-part answers as reliable as single-fact ones, at the
cost of extra time for those questions only.

### User Story 1 - Enumerated multi-part question returns correct per-part answers (Priority: P1)

An officer asks a question with explicit lettered parts, e.g. *"Identify the Principles
of Policy articles dealing with (a) the Islamic way of life, (b) promotion of local
Government institutions, and (c) participation of people in the Armed Forces."* Each
part is a single, answerable fact; the officer expects each part answered correctly and
labelled, not merged into one garbled sentence.

**Why this priority**: This is the worst-failing and most clearly-detectable class
(explicit `(a)(b)(c)` markers). Highest accuracy gain for the least detection risk.

**Independent Test**: Ask the (a)(b)(c) Principles-of-Policy question; verify the answer
states Article 31 (Islamic way of life), Article 32 (local Government), Article 39
(Armed Forces) — each correct and clearly attributed to its part.

**Acceptance Scenarios**:

1. **Given** a question containing `(a) … (b) … (c) …` enumerated parts, **When** the
   officer submits it, **Then** the answer contains one labelled block per part, each
   with the correct fact and a citation, and no part's fact bleeds into another.
2. **Given** the same question, **When** the answer is produced, **Then** every figure
   (article number, age, time period) in each part matches the source text verbatim.

---

### User Story 2 - Multi-article question answers each named provision correctly (Priority: P1)

An officer asks about two or more named provisions in one question, e.g. *"What property
rights does Article 23 grant, and what additional protection does Article 24 provide?"*
The officer expects Article 23 answered from Article 23 and Article 24 answered from
Article 24 — not one of them conflated with a neighbouring article.

**Why this priority**: Equally common, equally detectable (the question names the
provisions explicitly), and currently fails on the second provision.

**Independent Test**: Ask the Article 23/24 question; verify the Article 23 portion
describes property rights and the Article 24 portion describes its protection — neither
substituted with content from another article.

**Acceptance Scenarios**:

1. **Given** a question naming two or more provisions, **When** submitted, **Then** the
   answer contains one labelled block per named provision, each answered from that
   provision's text with a citation.
2. **Given** a question naming two provisions where only one is present in the corpus,
   **When** submitted, **Then** the present provision is answered and the absent one is
   explicitly reported as not found, rather than fabricated.

---

### User Story 3 - Single-fact questions are unaffected (Priority: P1)

An officer asks an ordinary single-fact question ("What does Article 9 guarantee about
security of person?"). The officer expects the same fast, accurate answer as today —
this feature must not slow down or change the common case.

**Why this priority**: A regression here would hurt the majority of queries. The
feature must be strictly additive: only multi-fact questions take the new path.

**Independent Test**: Ask any single-fact question from the existing 15-prompt eval;
verify the answer and the response time are unchanged from current behaviour.

**Acceptance Scenarios**:

1. **Given** a question with no enumeration markers and at most one named provision,
   **When** submitted, **Then** it is answered by the existing single-pass pipeline with
   no added latency.
2. **Given** a multi-fact-looking question that cannot be confidently split, **When**
   submitted, **Then** the system falls back to the single-pass pipeline rather than
   producing a malformed split.

---

### User Story 4 - Capable deployments bypass decomposition entirely (Priority: P1)

The app ships across hardware categories: small local models (0.5B / 1.5B) on
low-RAM CPU machines, and larger models (3B+ / GPU) on capable machines. A larger
model already answers multi-fact questions correctly in a single pass. On those
deployments, decomposition is pure overhead (extra latency, more passes) and must
**not** trigger — the multi-fact question is answered natively in one pass.

**Why this priority**: Decomposition is a *crutch for small models*. Applying it to a
capable model wastes time and adds failure surface for no benefit. The feature must be
self-disabling on capable deployments so the same binary behaves correctly everywhere.

**Independent Test**: On a deployment whose active generator is a large model (≥3B) or
a GPU tier, ask a multi-fact question; verify it is answered in a single native pass
(decomposition does not trigger) and the answer is correct.

**Acceptance Scenarios**:

1. **Given** the active generator is a capacity-limited small model (≈0.5B–1.5B) on a
   low-resource tier, **When** a recognised multi-fact question arrives, **Then**
   decomposition triggers.
2. **Given** the active generator is a larger model (≥3B) or a GPU-class tier, **When**
   the same multi-fact question arrives, **Then** decomposition does NOT trigger and the
   question is answered by the existing single-pass pipeline.
3. **Given** an operator override is set, **When** a question arrives, **Then** the
   override forces decomposition on or off regardless of the auto-detected category.

---

### Edge Cases

- **Capability gate at the boundary**: if the active model's size cannot be determined
  (e.g. a GGUF whose filename carries no size token), the system uses the hardware
  profile (low RAM / no discrete GPU → treat as capacity-limited) to decide, and the
  operator override always wins.
- **No confident split**: a question that looks multi-part but yields fewer than two
  valid sub-questions → fall back to single-pass (never ship a one-item "merge").
- **Citations across parts**: each sub-answer carries its own citations; the merged
  answer's citation markers must remain valid and unambiguous (a reader can map every
  `[n]` to a real source).
- **A sub-question finds nothing**: that part is reported as "not found in the retrieved
  passages" instead of being dropped or fabricated.
- **Cancellation**: if the officer cancels mid-answer, in-flight sub-questions stop and
  no partial/garbled answer is committed.
- **Very many parts**: a question whose parts exceed a sane cap is **not** answered as a
  truncated subset; it falls back to the single-pass pipeline (the whole question is
  answered in one pass rather than silently dropping parts).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect, using deterministic rules only, when a question is a
  multi-fact question of one of three recognised shapes: (a) it contains explicit
  enumeration markers `(a) … (b) …`; (b) it names two or more distinct provisions; or
  (c) it asks for **two facets of a single named provision**, signalled either by a count
  cue ("two"/"three"/"both") with a coordinated "… concerning/about X and Y" pair, or by a
  "…, and its {condition|exception|limitation|proviso|restriction}" qualifier. Shape (c)
  is conservative by design (requires the explicit cue) to avoid splitting ordinary
  single-fact questions that merely contain the word "and".
- **FR-002**: System MUST decompose a detected multi-fact question into an ordered list
  of single-fact sub-questions, one per enumerated part or named provision, preserving
  the original question's framing for each part.
- **FR-003**: System MUST answer each sub-question through the same retrieval-and-answer
  pipeline used for ordinary single-fact questions.
- **FR-004**: System MUST assemble the final answer by combining the sub-answers using a
  fixed template, with no model-generated rewriting of the combined text, so that facts
  and figures in each sub-answer are reproduced unchanged.
- **FR-005**: Each part of the merged answer MUST be clearly labelled (by its letter or
  named provision) and MUST carry the citation(s) that support it.
- **FR-006**: System MUST fall back to the existing single-pass behaviour whenever a
  question is not confidently detected as multi-fact, or when decomposition produces
  fewer than two valid sub-questions.
- **FR-007**: Single-fact questions MUST follow the existing path unchanged, with no
  added latency.
- **FR-008**: When a sub-question's answer cannot be grounded in the corpus, that part
  MUST be reported as not found rather than omitted or fabricated.
- **FR-009**: Citation markers in the merged answer MUST remain valid and consistently
  resolvable to their sources.
- **FR-010**: The decomposition path MUST honour query cancellation, stopping in-flight
  sub-questions without committing a partial answer.
- **FR-011**: System MUST gate decomposition on generator capability: it triggers ONLY
  when the active generator is a capacity-limited small model (parameter size at or below
  a configured threshold, defaulting to ≤1.5B so it covers the 0.5B and 1.5B deployments
  but not 3B+). On larger / GPU-class deployments decomposition MUST be disabled and the
  question answered by the single-pass pipeline.
- **FR-012**: When the active model's size is indeterminate, System MUST fall back to the
  hardware profile (low RAM and/or no discrete GPU ⇒ capacity-limited) to decide whether
  to gate decomposition on.
- **FR-013**: System MUST provide an operator override (configuration) that can force the
  decomposition path on or off irrespective of the auto-detected capability category.

### Key Entities

- **Multi-fact question**: an officer's question that asks for two or more independent
  facts; recognised by enumeration markers or by naming multiple provisions.
- **Sub-question**: a single-fact question derived from one part of a multi-fact
  question; carries its own retrieval, answer, and citations.
- **Merged answer**: the deterministic, template-assembled response combining all
  sub-answers, each labelled and cited.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the 15-prompt fundamental_rights eval, the targeted multi-fact
  questions (the Principles-of-Policy enumeration, the two-protections question, and the
  two-article property question) return the correct facts and figures for every part.
- **SC-002**: Single-fact questions in the same eval return the same answers as before
  this feature, with no measurable change in response time.
- **SC-003**: Every figure (article number, age, time period) in a merged multi-fact
  answer matches the source text verbatim — zero scrambled or invented numbers across
  the targeted questions.
- **SC-004**: For a multi-fact question where one named provision is absent from the
  corpus, the answer correctly reports that part as not found and still answers the
  present part(s).
- **SC-005**: No multi-fact question that the rules cannot confidently split produces a
  malformed answer; all such questions fall back to the single-pass result.
- **SC-006**: On a capable deployment (active generator ≥3B or a GPU-class tier), no
  multi-fact question triggers decomposition — every such question is answered in a
  single native pass, with no added latency from this feature.
- **SC-007**: The same application binary, run once on a small-model/low-resource
  deployment and once on a large-model/GPU deployment, exhibits decomposition in the
  former and single-pass in the latter for the identical multi-fact question — with no
  code change, driven only by the capability gate.

## Assumptions & Locked Decisions

These reflect decisions already made by the architect and bound the first cut:

- Detection and splitting are **rule-based only** — no model is used to decide whether
  to split or how to split.
- Decomposition is a **crutch for small models**, not a general feature. It is gated on
  generator capability and is **off by default for larger/GPU deployments**. The gate
  signal is the active model's parameter size (the app already parses 0.5/1.5/3/7 from
  the GGUF) with a hardware-profile fallback (RAM / discrete-GPU) when size is unknown,
  plus an operator override. Default threshold ≤1.5B (covers the shipped 0.5B and 1.5B
  small-model deployments; excludes 3B+).
- Merging is a **deterministic template** — no model rewrites or "polishes" the combined
  answer, because a model rewrite could corrupt facts/figures (the very failure this
  feature exists to prevent).
- First cut handles exactly two shapes: explicit `(a)(b)(c)` enumeration and
  multi-provision questions (2+ named provisions). Other multi-fact phrasings are out of
  scope for this cut and fall back to single-pass.
- Added latency for the multi-fact path is **accepted** ("a delayed right answer is
  preferred over a fast wrong one"); the single-fact path must not regress.
- The adjacent-provision confusion class (mislabelling a category, grabbing a
  neighbouring article on a *single*-fact question) is a different problem and is **not**
  addressed here.

## Out of Scope (first cut)

- Model-based question splitting.
- Model-based merging or readability rewriting.
- Fuzzy multi-fact detection without explicit markers or named provisions.
- Fixing single-fact adjacent-article confusion.
</content>
