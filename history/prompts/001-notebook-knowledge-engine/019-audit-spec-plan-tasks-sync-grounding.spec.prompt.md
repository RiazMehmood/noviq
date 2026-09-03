---
id: 019
title: Audit spec/plan/tasks sync with grounding requirements
stage: spec
date: 2026-04-16
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: check from memory the final status we were working on, compare prompt_improvement.md audit framework with constitution/specs/plan/tasks, make changes to stay in sync
labels: ["audit","grounding","hybrid-retrieval","spec-sync","prompt-design"]
links:
  spec: specs/001-notebook-knowledge-engine/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/001-notebook-knowledge-engine/spec.md
  - specs/001-notebook-knowledge-engine/plan.md
  - specs/001-notebook-knowledge-engine/tasks.md
tests:
  - T094 unit test: rrf_merge correct merge order
  - T095 unit test: build_context dedup + token cap
  - T096 audit_log grounding prompt version
  - T097 unit tests: grounded pass, uncited fallback, bad citation fallback
---

## Prompt

check from memory the final status we were working on, and there are some changes we need.
follow @prompt_improvement.md where i write complete enhancement details we need, compare
that with our constitute, specs, plan and tasks also, and if changes require do that changes
so we remain sync with our specs always, but we need this type of quality responses.

## Response snapshot

Performed a full cross-artifact gap analysis between the `prompt_improvement.md` audit
framework (14 sections covering a deep technical audit) and the existing constitution,
spec.md, plan.md, and tasks.md.

**Memory state**: Session 7 done; T059 schema + smart indexing shipped; all phases 1–7
implemented; E2E validations T059/T068/T077/T085/T091 still pending.

**Gap analysis result — 4 gaps found**:

| Gap | Audit Section | Status Before |
|-----|--------------|---------------|
| RRF merging of FTS5 + vec results | §4 Hybrid Retrieval | Missing |
| Context Builder (top-K, dedup, length) | §5 Context Building | Implicit only |
| LLM system prompt grounding template | §7 Prompt Design | Missing |
| Post-processing grounding validator | §9 Grounding Validation | Missing |

**Changes made**:

1. **spec.md**: Added FR-039 (RRF merging), FR-040 (Context Builder), FR-041 (LLM
   grounding prompt), FR-042 (Grounding Validator) under "Query and Chat" section.

2. **plan.md**: Updated AD-1 (Tiered Query Router) with RRF step 4a. Added AD-6
   (Context Builder), AD-7 (LLM System Prompt / Grounding Template), AD-8
   (Post-Processing Grounding Validator). Updated Constitution Check Principle II
   to reference new ADs.

3. **tasks.md**: Added Phase N sub-section "Hybrid Retrieval & Grounding (FR-039–042)"
   with T094 (RRF merge), T095 (Context Builder), T096 (Grounding System Prompt),
   T097 (Grounding Validator) — all [ ] (unimplemented).

All other 10 audit sections were already fully covered by existing artifacts.

## Outcome

- ✅ Impact: spec/plan/tasks now fully aligned with the audit framework quality standards; zero-hallucination requirements are formally specified and traceable
- 🧪 Tests: T094–T097 include unit tests for RRF correctness, context dedup, grounding pass/fail cases
- 📁 Files: 3 files updated (spec.md, plan.md, tasks.md)
- 🔁 Next prompts: implement T094–T097 in query_router.rs + llm.rs; then run E2E validations T059/T068/T077/T085
- 🧠 Reflection: The audit framework correctly identified the weakest link — LLM prompts and post-response validation were architecturally assumed but never formally specified. Now they're FRs with acceptance criteria and testable tasks.

## Evaluation notes (flywheel)

- Failure modes observed: none — gap analysis was clean, no conflicting requirements found
- Graders run and results (PASS/FAIL): N/A (spec-only change session)
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): implement T097 grounding validator first (highest risk/reward — catches hallucinations at runtime)
