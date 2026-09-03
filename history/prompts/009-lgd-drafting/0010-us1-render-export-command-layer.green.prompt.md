---
id: 0010
title: US1 render export command layer
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 008-query-decomposition
user: Riaz Mehmood
command: /sp.implement
labels: [feature-009, us1, drafting, citation-grade, render, export, acceptance-test]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: N/A
  adr: history/adr/0008-deterministic-citation-integrity.md
  pr: N/A
files:
  - src-tauri/src/services/drafting/render.rs
  - src-tauri/src/services/drafting/export.rs
  - src-tauri/src/commands/drafting.rs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/lib.rs
  - src-tauri/Cargo.toml
  - src-tauri/tests/lgd_drafting_acceptance.rs
  - src-tauri/tests/fixtures/lgd/rules.txt
  - src-tauri/tests/fixtures/lgd/case.txt
  - src-tauri/tests/fixtures/lgd/template.json
  - specs/009-lgd-drafting/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib drafting: 12 passed"
  - "cargo test --test lgd_drafting_acceptance: 1 passed"
---

## Prompt

continue with render and export commands

(followed by: "i think we need to run build after completion all tasks only, if we run
it after each step or task it will consume too much time")

Context: continuing /sp.implement for Feature 009 US1 (citation-grade draft from a
confirmed document set) after the Phase 1–2 foundation had landed. The render core
(`render_draft` with injected closures), `export.rs`, and the unit tests already existed;
the task was to finish + register the Tauri command layer, then prove the contract.

## Response snapshot

Registered the command layer and proved the US1 MVP end-to-end:
- Added `pub mod drafting;` to `commands/mod.rs`; registered `render_draft`, `export_draft`,
  `confirm_template`, `list_templates`, `get_template`, `delete_template` in the Tauri
  `invoke_handler` (lib.rs).
- `commands/drafting.rs`: `retrieve_scoped` performs a scoped `vec_distance_cosine` query
  restricted to the officer-confirmed `source_ids` (= the scoping mechanism, FR-016/018),
  returning `query_router::Citation` so the render helpers select deterministically (ADR-0008).
  `render_draft` fills cited slots from rules docs, `fact:`-prefixed cited slots from case docs,
  prose via a sanitized placeholder (model wiring left as an explicit TODO).
- Created the synthetic fixture (T011: `rules.txt`, `case.txt`, `template.json`) and the
  no-LLM acceptance gate (T013: `tests/lgd_drafting_acceptance.rs`, registered `[[test]]`).
- Honored the batch-build instruction: wrote the whole command batch + fixture + test, then ran
  exactly TWO cargo invocations (one `--lib drafting`, one `--test`) at the milestone.
- Silenced the lone dead-code warning on the `contains_provision_number` backstop.
- Updated tasks.md (T002/T011/T013–T020 with honest [~] on T017 prose model wiring deferred),
  CHANGELOG_DEV.md, and project memory.

## Outcome

- ✅ Impact: US1 MVP backend is complete and *proven* — a citation-grade draft renders from a
  template + confirmed set, with verbatim quotes, numbers lifted from chunk metadata (never
  generated), extractive facts, number-free prose, and refuse-or-flag on unanchored slots.
- 🧪 Tests: `cargo test --lib drafting` 12/12; `cargo test --test lgd_drafting_acceptance` 1/1
  (asserts SC-001, SC-002, FR-007, FR-008 with no live LLM). Full crate compiles.
- 📁 Files: render/export/command layer + fixture + acceptance test + tasks/changelog/memory.
- 🔁 Next prompts: US2 template ingestion (`ingest.rs`), US3 doc-set curation, then the Noviq UI
  (3-column layout, logo, Index-Folder-first-screen).
- 🧠 Reflection: injecting the retrieval + prose closures into `render_draft` kept the core
  deterministic, so the citation-grade contract is enforced by a fast unit/integration gate with
  zero model or DB dependency — exactly the property the zero-tolerance contract needs.

## Evaluation notes (flywheel)

- Failure modes observed: file-read tracking reset after compaction (re-read before Edit);
  a transient IDE cargo check briefly held no lock (verified clear before building).
- Graders run and results (PASS/FAIL): acceptance gate PASS (SC-001/SC-002/FR-007/FR-008).
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): wire the real `ModelSlot` into the prose path
  (T017) behind the existing `sanitize_prose` backstop, then re-run the acceptance gate.
