---
id: 0006
title: ADR llm ir pipeline
stage: misc
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.adr
labels: [adr, 006, llm-ir, grammar-constrained]
links:
  spec: specs/006-llm-pdf-importer/spec.md
  ticket:
  adr: history/adr/0010-llm-intermediate-representation-pipeline-grammar-constrained-and-pdf-image-only.md
  pr:
files:
  - history/adr/0010-llm-intermediate-representation-pipeline-grammar-constrained-and-pdf-image-only.md
tests:
  - none (ADR)
---

## Prompt

ADR C of the 4-primary batch: document Feature 006's LLM IR pipeline cluster — grammar-constrained (llguidance JSON-schema) generation + PDF/scanned/image-only scope, with trait IrBuilder/MistralIrBuilder, rule-based fallback, offline + bounded slot, sha cache + repair UI.

## Response snapshot

Created ADR-0010 "LLM Intermediate-Representation Pipeline — Grammar-Constrained and PDF/Image-Only" (Accepted). Decision cluster: llguidance constrained sampling for structurally-valid IR + two-stage validator + bounded retries; LLM path only for PDF/scanned/image (structured inputs short-circuit; indexer filter narrowed to Pdf|Image); graceful fallback to 005 rule-based parser; offline-only, Tier-4 worker released between docs; sha cache + partial-success repair UI. Pros/cons + 4 alternatives (free-text+lenient, LLM-for-everything, cloud, rule-only) rejected. Pulled real context from 006 plan.

## Outcome

- ✅ Impact: Documents the import subsystem's two headline decisions in one cluster ADR.
- 🧪 Tests: none (ADR)
- 📁 Files: history/adr/0010-llm-intermediate-representation-pipeline-grammar-constrained-and-pdf-image-only.md
- 🔁 Next prompts: ADR D (005 routing).
- 🧠 Reflection: Scope (PDF/image-only) and order (structured-first, ADR-0011) are complementary; documented as related ADRs.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): ADR significance checklist — PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): proceed to ADR D.
