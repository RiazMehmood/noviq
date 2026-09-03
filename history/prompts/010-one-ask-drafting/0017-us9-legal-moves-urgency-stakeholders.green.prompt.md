---
id: 0017
title: US9 — Legal analytical moves + urgency + auto-extracted stakeholders (per-move caps)
stage: green
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: per-move caps, create US9 for this and apply in one go
labels: [drafting, one-ask, secretariat-summary, citation-grade, compose-time]
links:
  spec: specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/drafting/one_ask/urgency.rs
  - src-tauri/src/services/drafting/one_ask/stakeholders.rs
  - src-tauri/src/services/drafting/one_ask/skeleton.rs
  - src-tauri/src/services/drafting/one_ask/state.rs
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - src-tauri/src/services/drafting/one_ask/compose.rs
  - src-tauri/src/services/drafting/one_ask/mod.rs
  - specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
tests:
  - cargo test --lib → 441 passed, 0 failed
  - one_ask suite 72/72 (urgency 5, stakeholders 3, classify_legal_move 1, per-move cap 1, skeleton 2)
---

## Prompt

Design (discussed first) then implement a secretariat summary house-style: (1) urgency, (2) legal
validation, (3) stakeholders, (4) stakeholder legal status, (5) normative procedure. Decisions:
moves 2/4/5 sit inside the Legal body; auto-extract stakeholders for moves 1 & 3; per-move caps;
"create US9 for this and apply in one go."

## Response snapshot

- Wrote US9 into the secretariat-summary spec (FR-014..018; SC-004 amended — per-move caps supersede
  the ≤3 global cap). Brainstormed the design first, surfaced 4 tensions (urgency-vs-no-advocacy,
  legal-validation-as-verdict, stakeholder fabrication, retrieval scope) and resolved them.
- Implemented 8 files, compose/retrieval-time (no re-index):
  - `urgency.rs` detect_urgency — verbatim time span or None (never generated).
  - `stakeholders.rs` extract_stakeholders — officer parties ∪ provision role-holders, generic lexicon.
  - `skeleton.rs` LegalMove + per-move caps (2/2/2) + LEGAL_CARD_BUDGET; ordered-move Legal guidance.
  - `state.rs` ProvisionCard.legal_move tag.
  - `research.rs` classify_legal_move; Legal fetches up to 6.
  - `verify.rs` per-move citation cap for Legal.
  - `compose.rs` urgency into Background, stakeholders into Legal; +2 params.
  - `mod.rs` wiring.
- Verified deterministic logic standalone (regex), then full `cargo test --lib` = 441/0.
- Side-fix: diagnosed and repaired the perpetually-failing `doc_profiles::manual` detection test
  (category error: heading strings asserted against type-detection patterns) — committed separately.

## Outcome

- ✅ Impact: Legal section becomes a staged legal analysis (basis → stakeholders → status →
  procedure); Background opens with factual urgency; stakeholders auto-named; citations bounded per
  move. Two-lane verbatim contract intact.
- 🧪 Tests: 441 passed / 0 failed; 12 new tests (11 US9 + the manual heading test).
- 📁 Files: 8 one_ask files + spec; manual.rs test fixed in a separate commit.
- 🔁 Next prompts: live-validate the ordered-move + grounding behaviour on the Canon-scanner ask
  (rebuild + relaunch; no re-index). v2: per-stakeholder status cap + retrieval; read-wide/cite-narrow.
- 🧠 Reflection: the deterministic foundation (urgency/stakeholder extraction, move classification,
  per-move caps) is fully unit-tested; the 7B's adherence to the ordered prompt is the only
  live-only surface — same pattern as the heading fixes.

## Evaluation notes (flywheel)

- Failure modes observed: urgency false positives ("within the rules"), stakeholder product leakage
  ("Canon scanners") — both guarded and tested. Move classification is approximate by design (only
  bounds caps, never produces a wrong citation).
- Graders run and results (PASS/FAIL): standalone regex harness PASS; cargo --lib PASS (441/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): if live drafts ignore the move order, split the Legal
  writer into per-move sub-prompts (accepting the latency cost) before adding per-stakeholder retrieval.
