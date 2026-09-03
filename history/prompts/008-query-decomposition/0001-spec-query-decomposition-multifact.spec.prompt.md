---
id: 0001
title: spec query decomposition multifact
stage: spec
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 008-query-decomposition
branch: 008-query-decomposition
user: Riaz Mehmood
command: /sp.specify
labels: [spec, query-decomposition, rag, multi-fact, intent-router]
links:
  spec: specs/008-query-decomposition/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/008-query-decomposition/spec.md
  - specs/008-query-decomposition/checklists/requirements.md
tests:
  - "n/a (spec stage)"
---

## Prompt

Architect requested (after switching from superpowers to SDD) a spec for query
decomposition of multi-fact questions in the legal RAG chat pipeline.

PROBLEM: The local Qwen2.5-0.5B generator is excellent at single-fact lookups (~9/15 on
the fundamental_rights grounded eval) but collapses on multi-fact questions — scrambles
numbers and self-contradicts when asked for 2+ facts at once (Q15 "(a) Islamic / (b)
local govt / (c) Armed Forces" → 1/29/32, correct 31/32/39; Q4 "two protections in
Article 14" → "13, 36"; Q9 "Article 23 and Article 24" → Art 24 half wrong). Retrieval
and prompting are already clean (V3 traces) — the failure is model capacity on multi-fact
synthesis, not context.

SOLUTION (decisions locked): detect multi-fact → decompose into single-fact sub-questions
→ run each through normal RAG → merge with a DETERMINISTIC TEMPLATE (no LLM merge).
Single-fact unaffected and fast.

SCOPE (locked): rule-based detection+split only; two shapes — explicit "(a)(b)(c)"
enumeration and multi-article (2+ named provisions via slots.section_paths); template
merge with labelled cited blocks; fall back to single-pass when no confident split.

NON-GOALS: LLM splitting; LLM merge/polish; fuzzy multi-fact; adjacent-article confusion
(Q12/Q13). CONSTRAINTS: latency up only for detected multi-fact ("delayed right > fast
wrong"); single-fact path unchanged; lives at intent_router/query_router seam; citations
stay valid. SUCCESS: Q4/Q9/Q15/Q6 correct facts; single-fact unchanged; no single-fact
latency regression.

## Response snapshot

Switched workflow from superpowers to SDD per architect. Created feature branch
`008-query-decomposition` (off 007, carrying the slot Fixes 1-3). Wrote spec.md with 3
P1 user stories (enumerated multi-part; multi-article; single-fact-unaffected), 10
functional requirements, edge cases (no-confident-split fallback, cross-part citations,
sub-question-not-found, cancellation, many-parts cap), 5 measurable success criteria
anchored to the 15-prompt eval, and an explicit "Assumptions & Locked Decisions" +
"Out of Scope" section capturing the architect's rule-based/template/2-shape decisions.
Generated requirements checklist — all items pass, zero [NEEDS CLARIFICATION] markers
(informed decisions drawn from the prior discussion).

## Outcome

- ✅ Impact: spec ready for planning; scope tightly bounded to the two detectable
  multi-fact shapes with single-pass fallback everywhere else.
- 🧪 Tests: n/a (spec stage).
- 📁 Files: spec.md, checklists/requirements.md.
- 🔁 Next prompts: user reviews spec → /sp.plan (then /sp.tasks → TDD /sp.implement).
- 🧠 Reflection: kept implementation choices in an Assumptions section rather than the
  FRs so the spec stays behaviour-focused while still recording the locked technical
  decisions.

## Evaluation notes (flywheel)

- Failure modes observed: prior V3 eval — 0.5B scrambles multi-fact numbers despite
  clean prompt+retrieval (the spec's reason for existing).
- Graders run and results (PASS/FAIL): requirements checklist PASS (all items).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): /sp.plan — design the detect→split→merge
  units and their tests at the intent_router seam.
</content>
