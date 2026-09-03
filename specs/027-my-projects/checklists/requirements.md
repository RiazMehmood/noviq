# Specification Quality Checklist: My Projects

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-05
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Validation Notes

**Iteration 1 — issues found and fixed:**

1. *Success criteria were technology-specific.* An earlier draft phrased SC-001 in terms
   of the function that computes visible documents. Rewritten as an observable citation
   count over a test set, verifiable without knowing how scoping is implemented.
2. *"Retrieval is scoped" was not testable as written.* Split into FR-010 (what may be
   retrieved), FR-011 (all three routes obey one rule), FR-012 (empty project), FR-013
   (server decides, client cannot widen) — each independently checkable.
3. *Deletion semantics were unstated* though the input implies document reuse across
   projects. Added FR-006/FR-007 and User Story 4, since a wrong reading here destroys
   data in a project other than the one being edited.
4. *"Hidden + unreachable" conflated two things.* FR-038 now states explicitly that
   removing the entry point is not the enforcement, matching the lesson that a UI toggle
   is never the gate.

**Iteration 2 — the three open questions resolved by the architect:**

- Privacy → disclose, don't change behaviour. FR-041 replaces the sole
  [NEEDS CLARIFICATION] marker; the residual risk is recorded as Risk 4 rather than
  hidden, because the whole weight now rests on that wording being clear.
- Toggle → per question, defaulting to excluded. FR-014/FR-017/FR-018 rewritten from the
  earlier per-project draft; User Story 3 and its independent test rewritten to match.
- Sharing → scope grew during the session from "single owner, sharing anticipated" to
  sharing at launch for both projects and chats.

**Iteration 3 — sharing added, and one conflict resolved before writing:**

The architect's answers initially conflicted: chat sharing was chosen as "full access,
they can continue the chat" and then described as recipients "can view the chat only".
Resolved by asking rather than guessing — the answer is view-by-link, fork-to-continue
(FR-029, FR-030). Guessing either way would have produced a materially different feature:
"continue" makes chat-sharing a back door into project access.

Project sharing resolved to **copy, not shared access**, which is materially simpler than
the shared-access model first offered — no permission levels, no invitations, no
concurrent editing (see Out of Scope). It rides directly on the existing index-once
link-many mechanism that Story 4 already required.

Two consequences of copy-on-share were not in the architect's answers and are recorded as
requirements rather than left implicit:

- FR-024 — a document, once shared, cannot be recalled from a copy. This follows
  necessarily from FR-007 (content survives while any project holds it) now extending
  across users, and it contradicts what users will expect. Disclosure at the moment of
  sharing is required.
- FR-009 — copies must not be charged against the recipient's storage allowance, since no
  additional bytes are stored. Left unstated, an implementation would plausibly charge
  them and make copying fail for no reason the user could understand.

**Verdict**: All checklist items pass. Ready for `/sp.plan`.

The specification is complete and internally consistent, but two structural facts
verified in the codebase this session should be carried into planning, because they
contradict what the design notes assumed:

- `notebooks` / `notebook_sources` are **live**, not dead F001 leftovers — they carry two
  meanings already (per-conversation stub, per-folder stub). Not free to repurpose.
- There is **no conversation table**. `query_sessions` / `query_messages` are unreferenced
  by the server; conversation identity is a client-minted string on `query_log` with no
  owning row. How a project chat's identity is recorded is the one genuine structural
  unknown left for `/sp.plan`.
