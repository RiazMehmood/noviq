---
id: 0011
title: US2 template ingestion proposed schema
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 008-query-decomposition
user: Riaz Mehmood
command: /sp.implement
labels: [feature-009, us2, drafting, template-ingestion, slot-schema, no-hardcoded-keywords]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: N/A
  adr: history/adr/0008-deterministic-citation-integrity.md
  pr: N/A
files:
  - src-tauri/src/services/drafting/ingest.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/lib.rs
  - specs/009-lgd-drafting/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib drafting: 16 passed (4 new ingest tests)"
---

## Prompt

continue with US2 template ingestion

## Response snapshot

Implemented US2 (template setup from an uploaded format), backend complete:
- `services/drafting/ingest.rs` — `propose_schema(text, instrument_type) -> ProposedSchema`. Pure,
  deterministically testable. Segments the format into heading-delimited regions with a GENERIC
  heading heuristic (`Label:` / ALL-CAPS-ish title / numbered outline) plus
  `heading_detector::regex_pass::scan` for legal markers. Classifies each region:
  provision-noun/number → cited (rule); currency/amount/figure → cited with a `fact:` category
  (extractive from case docs, matching `render.rs`); placeholder fill with neither signal → prose
  (flagged for review); fixed text → boilerplate (verbatim). Derives each cited slot's
  `required_source_category` from its OWN heading/body (salient tokens minus stopwords) — no
  hardcoded document names (honors diagnose-before-code). Low-confidence regions returned in
  `flaggedForReview`.
- `commands/drafting.rs` — `ingest_template` (extract via `parsers::{pdf,document,text}` by
  `FileFormat`, PDF/DOCX/TXT/MD only with a remediation error) returning `ProposedSchema`;
  `confirm_template` extended with optional `goldExampleRef` → commits gold METADATA only
  (`gold::record_gold_example`, SC-007). Registered `ingest_template` in the invoke_handler.
- Updated tasks.md (T022–T026), CHANGELOG_DEV.md, project memory.

## Outcome

- ✅ Impact: officers can turn their own Summary/Note formats into reusable slot schemas; the
  proposal is heuristic + officer-confirmed (the US2 human gate), and gold examples are recorded
  as metadata only (never indexed, SC-007).
- 🧪 Tests: `cargo test --lib drafting` 16/16 (4 new: blank-skeleton classification, amount→fact
  slot, proposed slots validate, generic heading detection). Full crate compiles.
- 📁 Files: ingest core + command wiring + tasks/changelog/memory.
- 🔁 Next prompts: US3 doc-set curation (`docset.rs` + `start_draft`/`update_doc_set`), then the
  Noviq UI (template manager T027, draft view, 3-column shell, Index-Folder-first-screen).
- 🧠 Reflection: keeping the provision/financial lexicons confined to a PROPOSAL the officer
  confirms (not as authoritative routing keys) satisfies the no-hardcoded-keywords feedback while
  still giving a useful first guess; the authoritative categories are the ones the officer approves.

## Evaluation notes (flywheel)

- Failure modes observed: two trivial warnings (unused `mut` closure; unused imports before the
  command was added) — both cleared in the same batch before the single milestone build.
- Graders run and results (PASS/FAIL): drafting lib suite PASS (16/16).
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): run `ingest_template` against a real Sindh Summary
  skeleton (once the user supplies gold) and inspect the proposed schema's flagged regions to tune
  the cited-vs-prose heuristic thresholds.
