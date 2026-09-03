---
id: 0009
title: LGD drafting foundation implemented
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.implement
labels: [green, lgd-drafting, backend, migration, summary-first]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - src-tauri/src/db/migrations/021_lgd_drafting.sql
  - src-tauri/src/services/drafting/mod.rs
  - src-tauri/src/services/drafting/slot.rs
  - src-tauri/src/services/drafting/template.rs
  - src-tauri/src/services/drafting/gold.rs
  - src-tauri/src/services/drafting/ingest.rs
  - src-tauri/src/services/drafting/docset.rs
  - src-tauri/src/services/drafting/gaps.rs
  - src-tauri/src/services/drafting/render.rs
  - src-tauri/src/services/mod.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib drafting: 6 passed / 0 failed (slot x3, template x2, gold x1)"
---

## Prompt

/sp.implement for Feature 009. Mid-session the user clarified scope: summary/proposal is separate from noting — asked whether we're doing summary or both. Decision (go with recommendation): Summary/Proposal first, Noting deferred to a second template on the same engine (instrument_type stays). Build foundation, verify, keep DB safe.

## Response snapshot

Implemented and verified Phases 1–2 (foundation) of tasks.md:
- Confirmed build target-dir is on New Volume (root at 99%→cleaned to 93% in a prior turn); CARGO target not on root.
- Studied wipe-on-mismatch in db/schema.rs → deliberately did NOT bump TARGET_SCHEMA_VERSION (16) so the user's indexed corpus is NOT wiped; migration 021 is additive and sqlx applies it incrementally.
- Wrote migration 021 (template_schema/template_slot/gold_example) + drafting module (slot, template CRUD, gold storage with SC-007 no-retrieval guard, working types) + 4 documented stubs (ingest/docset/gaps/render) + registered the module.
- Scope decision recorded in spec (Clarifications + FR-001) and tasks.md (Summary first, Note deferred, same engine).
- `cargo test --lib drafting` → **6/6 pass**; full crate compiles (pre-existing warnings only, none from new code).
- Marked T001, T006–T010 done in tasks.md; added CHANGELOG_DEV entry.
- Saved memory: user prefers recommendations over the AskUserQuestion picker.

## Outcome

- ✅ Impact: Verified, DB-safe foundation for the drafting feature; Summary-first scope locked; ready to build the US1 render path.
- 🧪 Tests: 6 new lib tests green (slot validation, template roundtrip + overwrite, gold no-retrieval).
- 📁 Files: migration 021 + services/drafting/* + services/mod.rs + CHANGELOG_DEV.md.
- 🔁 Next prompts: implement US1 render path (T015–T019) + acceptance fixture (T011/T013); then US2/US3/US4 + Noviq UI.
- 🧠 Reflection: Keeping instrument_type=summary|note means Noting is a template, not a rewrite — Summary-first costs nothing later.

## Evaluation notes (flywheel)

- Failure modes observed: none; clean compile + green tests.
- Graders run and results (PASS/FAIL): cargo test --lib drafting — PASS (6/6).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T015 cited-slot fill reusing decompose::resolve_provision_label over a synthetic fixture, asserting SC-001/SC-002 before wiring the command.
