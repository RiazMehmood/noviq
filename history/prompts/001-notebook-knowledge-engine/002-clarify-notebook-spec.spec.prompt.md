---
id: "002"
title: Clarify Notebook Knowledge Engine Spec
stage: spec
date: 2026-04-13
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: /sp.clarify
labels: ["spec","clarification","hardware-adaptive","deduplication","state-lifecycle","import-export"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/001-notebook-knowledge-engine/spec.md
tests:
  - N/A
---

## Prompt

/sp.clarify (run against specs/001-notebook-knowledge-engine/spec.md)

Preceded by constitution sync review which identified 3 gaps (FR-014 missing
Tier 0 cache, FR-011 missing battery threshold, missing FR for low storage).
All 3 gaps were patched before clarification ran.

## Response snapshot

5 questions asked and answered. All critical ambiguities resolved.

**Q1**: Analysis templates on 8 GB hardware (no Tier 4)?
→ C: Templates available on all tiers; best available model used; quality notice shown.
→ Added FR-027a.

**Q2**: Notebook archive export/import — both in v1?
→ A: Bidirectional — export and import both in v1; outdated archives re-index automatically.
→ Added FR-036a, FR-036b; updated Assumptions.

**Q3**: Source indexing state lifecycle?
→ Suggested accepted: `pending → processing → indexed | failed`; re-indexing re-enters processing.
→ Updated Source entity definition; updated FR-006 with retry action.

**Q4**: Same file in notebook + global index — how to handle?
→ A: Path-based deduplication — one index entry per unique file path, shared across contexts.
→ Added FR-035a; updated Edge Cases.

**Q5**: Office persona/domain presets in notebook creation?
→ A: No presets in v1 — plain notebooks only; persona behavior scoped to App 2.
→ Updated Assumptions with explicit out-of-scope declaration.

## Outcome

- ✅ Impact: Spec fully clarified; 5 ambiguities resolved; 5 new/updated FRs added
  (FR-006, FR-027a, FR-035a, FR-036a, FR-036b); Source state lifecycle defined;
  deduplication policy explicit; import/export complete.
- 🧪 Tests: N/A (specification stage)
- 📁 Files: specs/001-notebook-knowledge-engine/spec.md
- 🔁 Next prompts: /sp.plan
- 🧠 Reflection: The hardware-adaptive template question (Q1) was the highest-impact
  clarification — it forced an explicit UX decision (never lock features, always
  show quality notices) that directly shapes the plan's degradation architecture.
  The import/export gap (Q2) was a clean half-feature catch that would have been
  expensive to add post-planning.

## Evaluation notes (flywheel)

- Failure modes observed: None
- Graders run and results (PASS/FAIL): All 5 questions answered; spec updated
  incrementally after each answer; no contradictions introduced; Clarifications
  section properly structured.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): Proceed to /sp.plan
