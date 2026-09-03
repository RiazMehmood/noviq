---
id: 0004
title: ADR deterministic citation integrity
stage: misc
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.adr
labels: [adr, citation, 008, 009, cross-cutting]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
tests:
  - none (ADR)
---

## Prompt

/sp.adr deterministic-citation-integrity — first of 4 primary ADRs the user asked to resolve before /sp.tasks. Document the metadata-driven extractive citation + refuse-or-flag decision spanning Feature 008 (Fix B resolve_provision_label selector, NUMBER_UNVERIFIED) and Feature 009 (drafting cited slots reuse the selector). V4 proved a 0.5B confabulates a number even with the correct labelled chunk present; alternatives = generative fill (rejected), LLM merge/rewrite (rejected), hardcoded per-template FTS queries (rejected), trust rank-1 chunk (rejected). Constitution XI alignment.

## Response snapshot

Created ADR-0008 "Deterministic Citation Integrity for Answers and Drafts" (Accepted). Decision: numbers/citations/quoted rule text produced deterministically from chunk metadata/text, never by a model; shared `resolve_provision_label` title-match-not-rank selector + refuse-or-flag sentinel across 008 (Q&A) and 009 (drafting cited slots; fact slots extractive from case docs; prose slots number-free). Positive (un-hallucinatable, single enforcement point, deterministic acceptance tests, safe degrade), Negative (more extractive/less fluent, depends on retrieval+label quality, threshold tuning). 4 alternatives with rationale. References plan R1/R2/R7, ADR-0007, V4/V5 eval evidence.

## Outcome

- ✅ Impact: The cross-cutting citation contract is now a documented Accepted decision; de-risks /sp.tasks for 009 (cited-slot path references it).
- 🧪 Tests: none (ADR)
- 📁 Files: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
- 🔁 Next prompts: ADR B (008 decomposition gating), then C (006 LLM-IR), D (005 routing), then /sp.tasks.
- 🧠 Reflection: Grouping 008's Fix B and 009's slot-schema citation path into one ADR keeps the contract singular (per CLAUDE.md "group related decisions").

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): ADR significance checklist — PASS (clustered cross-cutting decision, ≥1 alternative with rationale, pros+cons, references).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): proceed to ADR B.
