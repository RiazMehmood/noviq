---
id: 0002
title: LGD drafting spec clarifications
stage: spec
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.clarify
labels: [spec, clarify, lgd-drafting]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/009-lgd-drafting/spec.md
tests:
  - none (spec/clarify stage)
---

## Prompt

/sp.clarify (no extra args). Run the structured ambiguity scan over specs/009-lgd-drafting/spec.md, ask up to 5 high-impact clarification questions one at a time, and integrate answers into the spec.

User answers given: Q1=A, Q2=A, Q3=A, Q4=A.

## Response snapshot

Scanned spec against the clarification taxonomy; most categories Clear. Asked and integrated 4 high-impact clarifications (added a `## Clarifications` → `### Session 2026-06-08` section):

1. Output rendering form → **structured, labelled, cited blocks per slot** for the first cut; layout-faithful document reproduction deferred. (Tightened the Assumption.)
2. Template upload formats → **same formats the existing indexer accepts** (PDF, DOCX, TXT, Markdown), reusing the extraction pipeline. (Added FR-009a.)
3. Gap-detection granularity → **kind/category of source** (not a specific filename); officer curation resolves the category to the actual document. (Tightened FR-011 + Assumption.)
4. Missing-document drop location → **the existing watched corpus/indexed folder** (reuse current file-watch). (Tightened FR-019.)

Stopped at 4 questions; remaining open areas (latency targets, template edit/version/delete lifecycle, cited-slot retrieval mechanics) deferred to /sp.plan as they don't change scope or acceptance tests.

## Outcome

- ✅ Impact: Spec de-risked on the four points that most affect rendering, ingestion, data model, and watcher wiring; ready for /sp.plan.
- 🧪 Tests: none (spec stage)
- 📁 Files: specs/009-lgd-drafting/spec.md (Clarifications section + FR-009a, FR-011, FR-019, Assumptions updated)
- 🔁 Next prompts: /sp.plan
- 🧠 Reflection: All four answers favored maximum reuse of existing infrastructure (indexer, extraction, file-watch, extractive output shape), keeping the feature a composition rather than new plumbing.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in /sp.plan, settle cited-slot retrieval mechanics (how a required-source category maps to a retrieval query + chunk selection) and a drafting latency target (speed-first per project preference).
