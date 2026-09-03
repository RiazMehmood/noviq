---
id: 0002
title: 012 doc-intelligence tasks
stage: tasks
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.tasks
labels: [tasks, ingestion, drafting, citation-grade, sdd]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/012-doc-intelligence-production/tasks.md
tests:
  - "cargo test --lib parsers:: heading_detector:: structure_parser:: structured_importer:: drafting::one_ask (G2 golden gate, per-wave)"
---

## Prompt

proceed to /sp.tasks

## Response snapshot

Generated `specs/012-doc-intelligence-production/tasks.md` — 30 tasks organized
by the three re-index waves and mapped to the five user stories:

- **Setup (T001–T002):** additive migration `003_chunks_provenance.sql`; G2
  golden-fixture harness scaffold.
- **Foundational (T003–T004):** shared `BlockClass`/`LabelStatus`/`ChromeReason`
  enums; extend chunk persist struct + INSERT with provenance fields.
- **US1 chrome-strip (T005–T009):** `parsers/chrome.rs` (positional-repetition,
  margin, form-field; provision-keep); wire into `pdf.rs`; supersede the
  downstream `is_boilerplate` flag; chrome golden fixtures.
- **US2 labels+provenance (T010–T013):** resolve `unit_label` from hierarchy;
  kill silent synthetic-id (`m66`) → `label_status='unresolved'`; provision
  golden fixtures.
- **US3 heading detection (T014–T018):** numbering-sequence prior, boundary-stop,
  promote `detect_label_mismatches` to a split action, confidence; heading fixtures.
- **US4 deterministic drafting (T019–T022):** slot+template frames, no LLM prose,
  decidable whitelist verify (Wave B, no re-index).
- **US5 table engine (T023–T027):** projection-profile column bands, wrapped
  continuation, cross-page stitch, confident-only fallback; Delegation fixtures.
- **Polish (T028–T030):** extend chrome exclusion to chat `query_router`;
  CHANGELOG per wave; MEMORY resume update.

Dependency graph + parallel opportunities + MVP-first strategy (MVP = Wave A)
included. Format validated: every task has checkbox + ID + (story label where
applicable) + file path.

## Outcome

- ✅ Impact: Feature 012 is task-complete and immediately executable; Wave A / US1
  is the first MVP slice. `/sp.implement` unblocked.
- 🧪 Tests: G2 golden gate + per-module cargo tests defined as the per-wave gate
  (tests included because the spec makes G2 a release gate).
- 📁 Files: tasks.md.
- 🔁 Next prompts: /sp.implement (start Wave A: T001→T002→T003→T004→US1), or
  /sp.analyze for cross-artifact consistency, or /sp.adr to record the major
  decisions first.
- 🧠 Reflection: schema (provenance columns) hoisted to Foundational because
  US1/US2/US3/US5 all write into it; this keeps Wave A's single-re-index
  sequencing clean.

## Evaluation notes (flywheel)

- Failure modes observed: none (task generation).
- Graders run and results (PASS/FAIL): format validation PASS (checkbox/ID/story/path).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): begin T001 (migration) + T002 (golden
  harness) under /sp.implement; confirm migration applies on live PG.
