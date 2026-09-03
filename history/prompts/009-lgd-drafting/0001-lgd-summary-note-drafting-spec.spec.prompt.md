---
id: 0001
title: LGD summary note drafting spec
stage: spec
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.specify
labels: [spec, lgd-drafting, citation-grade, templates]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/009-lgd-drafting/spec.md
  - specs/009-lgd-drafting/checklists/requirements.md
tests:
  - none (spec stage)
---

## Prompt

User idea (drafting surface for Sindh Local Government officers): give the user an option to upload the format/template they want, and the app drafts summaries/proposals accordingly. Ship the app with our own built template, then let the user upload 2-3 formats we make necessary; the app uses those as templates and generates summaries.

Refined through discussion into:
- Template type = hybrid (blank skeleton OR real filled example), normalized into one internal slot schema; filled examples additionally kept as gold/style reference.
- Template ingestion = auto-detect a proposed slot schema (boilerplate / cited slot / prose slot) + per-cited-slot required-source, with a one-time officer confirmation.
- Two input channels only: (1) the existing indexed corpus for ALL substantive docs (rules + case docs) — nothing uploaded per draft; (2) the template channel (special upload, not indexed).
- Drafting flow: no manual scope (whole corpus by default); intent Q&A; app proposes a document set; officer curates by number (this curation IS the scoping mechanism); gap detection from the template's required-source descriptors → "index this document in <folder>" with openable link; officer either supplies it (folder watched → set updates → notified) or declares it unavailable and proceeds (affected slots flagged); final confirmation → execute → output.
- Scope (first cut): BOTH Summaries and Notes.
- Citation-grade contract throughout: model never generates numbers/citations; deterministic verbatim lift + attribution; refuse-or-flag on unanchored cited slots.

Instruction: use SDD (not superpowers). Then: "per-slot required-source works, write the spec."

## Response snapshot

Created branch `009-lgd-drafting` (next number after 008) and wrote `specs/009-lgd-drafting/spec.md`:
- 4 prioritized, independently-testable user stories: US1 (P1) produce citation-grade draft from confirmed set; US2 (P2) template setup from uploaded format; US3 (P2) curate document set; US4 (P3) detect/resolve missing required documents.
- 22 functional requirements grouped (instruments/output, citation-grade contract, templates, corpus/inputs, drafting flow).
- Key entities, edge cases, assumptions, dependencies, and 7 measurable success criteria (100% verbatim citations, 100% refuse-or-flag coverage, zero correction forwardable drafts, no gold leakage).
- Output rendering form left as a documented Assumption (structured cited blocks first cut; layout-faithful reproduction future) flagged for `/sp.clarify` — no blocking NEEDS CLARIFICATION markers.
- Wrote quality checklist `checklists/requirements.md`; all items pass.

## Outcome

- ✅ Impact: First-cut spec for the headline citation-grade drafting surface (extends the Q&A citation contract to drafting official instruments); ready for clarify/plan.
- 🧪 Tests: none (spec stage)
- 📁 Files: spec.md, checklists/requirements.md
- 🔁 Next prompts: `/sp.clarify` (confirm output rendering form + gap-detection granularity) then `/sp.plan`.
- 🧠 Reflection: The document-list curation doubling as the scope mechanism, and per-cited-slot required-source powering gap detection, were the two design keys that fell out of the discussion.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (no implementation).
- Graders run and results (PASS/FAIL): spec quality checklist — PASS (all items).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in `/sp.clarify`, pin the output rendering form (structured blocks vs layout-faithful) and confirm gap-detection granularity (kind-of-document vs named file).
