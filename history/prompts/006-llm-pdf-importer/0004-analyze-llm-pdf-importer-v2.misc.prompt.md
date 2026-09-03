---
id: "0004"
title: analyze llm pdf importer v2
stage: misc
date: 2026-05-29
surface: agent
model: claude-opus-4-7
feature: 006-llm-pdf-importer
branch: 006-llm-pdf-importer
user: engr.riaz2010@gmail.com
command: /sp.analyze
labels: [analyze, consistency-check, read-only, feature-006]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket: ""
  adr: ""
  pr: ""
files: []
tests: []
---

## Prompt

Analyze feature 006 (LLM PDF→IR importer) on branch 006-llm-pdf-importer. Cross-check spec.md, plan.md, research.md, data-model.md, contracts/*, tasks.md for consistency, completeness, ambiguity, duplication, untestable claims, and gaps. Non-destructive — report only.

## Response snapshot

Read-only cross-artefact analysis of all 7 design files. **0 critical, 2 high, 6 medium, 7 low findings**. FR coverage 29/30 = 96.7%. Constitution gates 7 PASS / 2 JUSTIFIED / 1 GAP.

**HIGH-severity items requiring fix before `/sp.implement`:**

- **G1 (FR-024 not tasked)** — spec mandates "one-at-a-time + queue position visible"; no task implements either the queue serialization wrapper or the queue-position event/UI. Fix: add backend queue task OR narrow FR-024 to drop queue-position visibility.
- **G2 (Constitution VII gap)** — plan claims `MistralIrBuilder::available()` returns false on hardware tier < 3; T021 only specifies GGUF-path check. Fix: extend T021 to consult `hardware_detector`.

**MEDIUM items (worth fixing pre-impl):**

- C3, C4 — SC-001 / SC-003 corpus is unbound. Pin to avoid cherry-picking on acceptance day.
- C1 — 3/2 retry bound lives only in plan+tasks, not spec; cosmetic.
- C2 — chunker (T017) and converter_from_constitution (T013) have no unit-test tasks.
- A1 — FR-018 sticky-repair match key doesn't define `parent_breadcrumb` precisely.
- A3 — chunker output `breadcrumb` type undefined.
- A4 — FR-007 "≥90% of source token count" — tokenizer unspecified.

**LOW (defensible to defer):**

- I2 — possible duplicate `ALTER TABLE sources ADD COLUMN extraction_method` between migrations 016 and 018; grep before SQL writes.
- I1, I3, A2, D1, T1, O1, U2 — cosmetic or already-passively-handled.

**Constitution re-check:** I, II, IV (justified), V, VI (n/a), VIII PASS; III JUSTIFIED + A2 notes passive eviction is fine; **VII GAP** (G2).

## Outcome

- ✅ Impact: feature 006 is implementation-ready after 2 HIGH-severity scope clarifications. No critical issues, no constitution violations beyond documented justifications.
- 🧪 Tests: validator + acceptance + cache-lifecycle covered. Adding chunker + converter unit tests (C2) closes residual regression risk.
- 📁 Files: no writes (read-only command).
- 🔁 Next prompts: optional one-shot edit to tasks.md (G1, G2) + spec.md (C3, C4) — then `/sp.implement` Phase 1+2.
- 🧠 Reflection: the spec-vs-tasks gap on FR-024 (queue-position visibility) is the kind of thing a spec-driven workflow is designed to catch. Worth pinning before code lands.

## Evaluation notes (flywheel)

- Failure modes observed: 1 FR entirely unimplemented in tasks (FR-024); 1 constitutional claim asserted in plan but not enforced in tasks (Principle VII).
- Graders run and results (PASS/FAIL): FR coverage 96.7% PASS; Constitution 7/8 PASS, 1 GAP.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): after G1/G2 fixes, re-run `/sp.analyze` and verify coverage is 30/30 + Constitution 8/8.
