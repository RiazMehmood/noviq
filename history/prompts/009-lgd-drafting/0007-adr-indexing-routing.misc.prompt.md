---
id: 0007
title: ADR indexing routing
stage: misc
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.adr
labels: [adr, 005, 006, indexing, routing]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket:
  adr: history/adr/0011-indexing-routing-structured-importer-primary-llm-ir-fallback.md
  pr:
files:
  - history/adr/0011-indexing-routing-structured-importer-primary-llm-ir-fallback.md
tests:
  - none (ADR)
---

## Prompt

ADR D (final of the 4-primary batch): document the indexing-routing decision — direct structured importer (005) primary, LLM IR builder (006) secondary fallback; already-structured inputs short-circuit first; LLM only for unstructured PDF/image when model available. Corrects the original LLM-primary ordering that stalled on text PDFs.

## Response snapshot

Created ADR-0011 "Indexing Routing — Structured Importer Primary, LLM IR Fallback" (Accepted). Decision: JSON sidecar/Markdown/Txt/Code short-circuit first; rule-based structured importer runs next; LLM IR reached only for unstructured PDF/image and only when the model is available (routing fix commit a516d93f). Pros (kills 7B-on-text-PDF stall, frees model slot, deterministic-first, layers with ADR-0010), cons (depends on detection quality, more parts, heuristic upkeep). 4 alternatives rejected.

## Outcome

- ✅ Impact: Completes the 4-primary ADR set (0008–0011); ingestion ordering now documented alongside scope (0010).
- 🧪 Tests: none (ADR)
- 📁 Files: history/adr/0011-indexing-routing-structured-importer-primary-llm-ir-fallback.md
- 🔁 Next prompts: /sp.tasks for 009.
- 🧠 Reflection: 005 has no specs/ dir; ADR anchors to the shared indexer dispatch + the routing-fix commit instead.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): ADR significance checklist — PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): /sp.tasks for Feature 009 (sequence US1 first).
