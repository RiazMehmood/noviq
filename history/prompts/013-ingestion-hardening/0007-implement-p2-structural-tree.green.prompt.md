---
id: 0007
title: Implement P2 structural tree (Phase 4 — D2/D3/D4/D10)
stage: green
date: 2026-06-20
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: engr.riaz2010@gmail.com
command: "recall memory and start working on feature 13 remaining phases"
labels: [ingestion, structure-parser, heading-detection, US1]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/doc_profiles/manual.rs
  - src-tauri/src/services/heading_detector/regex_pass.rs
  - specs/013-ingestion-hardening/tasks.md
  - MODULAR_IMPROVEMENT_LOG.md
tests:
  - "cargo test --lib regex_pass: 18/18 (2 new)"
  - "cargo test --lib: 496 passed / 2 failed (2 = known pre-existing Phase-3 chrome footnote-leak tests)"
---

## Prompt

recall memory and start working on feature 13 remaining phases

## Response snapshot

Recalled state from MEMORY.md + MODULAR_IMPROVEMENT_LOG.md + tasks.md. Next locked
phase = Phase 4 (P2 structural tree). Diagnosed the open D2 defect: lost Articles
9/33/34/35/39 were a catchline↔body double-match collision (each provision prints
its number on the catchline and again on a plain-prose body; both match the
bare-number pattern → two same-`unit_path` nodes collide → provision dropped;
articles whose body opens with `(1)` matched once and survived).

Implemented: T019 alpha-suffix ordinals `\d+[A-Z]?` (10A/19A/25A); T021 arabic
`CHAPTER 1./2.` detection; T022 collapse consecutive same-label headings in
`scan_with_profile`; T023 falls out (recovered provisions become `article`
unit_type). Added 2 unit tests. T020 already satisfied by existing sequence
filter. T024 (25/25A order flag) deferred as a non-invasive refinement.

## Outcome

- ✅ Impact: All of Articles 7–40 (incl. 10A/19A/25A) now detectable as `article`
  nodes; real Chapter 1/2 nodes; 9/33/34/35/39 recovered from `clause`/Part-II.
- 🧪 Tests: regex_pass 18/18; full lib 496 pass / 2 fail (the 2 are the known
  Phase-3 footnote-leak `chrome.rs` tests, untouched by this change).
- 📁 Files: manual.rs (patterns), regex_pass.rs (collapse + tests), tasks.md,
  MODULAR_IMPROVEMENT_LOG.md.
- 🔁 Next prompts: live re-index verify; P1 footnote-leak refinement (2 red →
  green); P3 vocabulary (Section→Article).
- 🧠 Reflection: the "lost" articles were never actually dropped by the sequence
  filter — they were mis-typed survivors; reading the baseline TSV pinned the real
  cause faster than re-reasoning the regex.

## Evaluation notes (flywheel)

- Failure modes observed: 2 pre-existing Phase-3 chrome footnote tests red (footnote
  text leaking into provision body) — separate workstream.
- Graders run and results (PASS/FAIL): regex_pass lib gate PASS; full lib gate
  FAIL only on the 2 known Phase-3 tests.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): build app + force-pending re-index of
  the 3-doc control set; diff Article 7–40 presence + Chapter nodes vs baseline.
