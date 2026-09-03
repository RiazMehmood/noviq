---
id: 0020
title: Clarify ingestion rewrite gap semantics
stage: spec
date: 2026-06-10
surface: agent
model: claude-fable-5
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: riaz
command: /sp.clarify
labels: [clarify, spec-amendment, template-ingestion, gap-detection, option-c]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - specs/009-lgd-drafting/spec.md
tests:
  - none (spec-only change)
---

## Prompt

Amend feature 009-lgd-drafting: scope two diagnosed defects — (1) template ingestion rewrite via Option C (Mistral-7B free-text segmentation + deterministic verbatim-verify gate, replacing the heading-heuristic ingest.rs that produced garbage slots on real instruments); (2) gap-detection coverage is title-token based and must become content/semantic-aware (embed required_source_category vs confirmed-doc chunks)

(Preceded in-session by: "skip the 0.5B, lock Mistral-7B and run /sp.clarify" — model decision locked after the 2026-06-10 Option-C ingestion spike: Mistral-7B free-text best, noviq_qwen3b unfaithful, llguidance grammar path crashes.)

## Response snapshot

Ran the /sp.clarify loop: 4 questions asked, 4 answered, integrated incrementally into spec.md under a new `### Session 2026-06-10` Clarifications block.

- Q1 (gap detection, two-tier corpus search) → A: category covered by an un-confirmed indexed doc = add-this-document suggestion (one-turn accept); only category covered by nothing in corpus = true gap. → FR-019 rewritten (also encodes content/semantic coverage vs titles, locked by user input); US4 gained a suggestion-tier acceptance scenario.
- Q2 (fact-slot gaps) → B: officer-supplied channel — fact gap prompts officer to point to a case doc OR type the value; typed value rendered with explicit "officer-supplied" attribution, never dressed as a citation. → FR-008 amended + new FR-008a.
- Q3 (verify-gate rejection) → A: flag-for-review — rejected region surfaced in schema-confirmation step as low-confidence with rejection reason; never silently dropped/accepted. → FR-010 tightened (boilerplate exclusion, inline + hyphenated citation detection) + new FR-010a (model-assisted segmentation + deterministic verbatim-verify gate, one-time offline).
- Q4 (model unavailable/unparseable) → B: refuse — ingestion fails with actionable error naming the missing model; heuristic-only fallback removed. → new FR-010b.

## Outcome

- ✅ Impact: Both live-test defects (garbage ingestion slots on real instruments; title-token gap over-reporting) now have spec-level contracts; Option C (local-model segmentation + deterministic gate) is in the spec as FR-010a/FR-010b; gap detection is content-semantic and two-tier per FR-019.
- 🧪 Tests: none run (spec-only); acceptance scenarios updated in US4.
- 📁 Files: specs/009-lgd-drafting/spec.md (Clarifications session 2026-06-10, FR-008/008a, FR-010/010a/010b, FR-019, US4 scenarios).
- 🔁 Next prompts: /sp.plan (refresh plan.md for ingestion rewrite + semantic gap coverage) then /sp.tasks; ADR-worthy decision (model-assisted ingestion w/ deterministic gate) to surface after planning.
- 🧠 Reflection: The spike evidence made every recommendation land on first ask — all 4 answers accepted as recommended/first option. Deferred to plan: embedding-similarity threshold for coverage, retry count before refusal, Mistral-7B as named model (spec stays model-agnostic, "local model").

## Evaluation notes (flywheel)

- Failure modes observed: none in this session; prior failure mode being fixed = heuristic segmentation producing prose-fragment categories and false cited slots on real instruments.
- Graders run and results (PASS/FAIL): n/a (spec stage).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): /sp.plan amendment scoping ingest.rs rewrite + gaps.rs semantic coverage.
