---
id: 0023
title: Phase D E defect fix tasks
stage: tasks
date: 2026-06-10
surface: agent
model: claude-fable-5
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: riaz
command: /sp.tasks
labels: [tasks, phase-d, phase-e, ingestion-rewrite, semantic-gaps, officer-supplied-facts]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: null
  adr: history/adr/0012-model-assisted-template-ingestion-with-deterministic-verbatim-verify-gate.md
  pr: null
files:
  - specs/009-lgd-drafting/tasks.md
tests:
  - planned: src-tauri/tests/lgd_ingest_gate.rs (T068), gaps.rs/render.rs/session.rs unit rewrites (T066/T067/T076/T077)
---

## Prompt

Append Phase D (Option-C ingestion rewrite, FR-010/010a/010b, migration 024) and Phase E (two-tier semantic gap coverage FR-019 + officer-supplied fact channel FR-008a) tasks to the existing 009-lgd-drafting tasks.md per the amended plan — do not regenerate existing tasks T001–T063

## Response snapshot

Appended **20 tasks (T064–T083)** to `specs/009-lgd-drafting/tasks.md`; T001–T063 untouched. Code surfaces were surveyed first so every task carries exact file paths, function names, and line anchors (`ingest.rs propose_schema/segment_regions/classify`, `gaps.rs category_covered:31/detect_gaps:47`, `commands/drafting.rs ingest_template:862/gaps_for:543/update_doc_set:559/resolve_gap:584/refresh_doc_set:623/retrieve_scoped:307`, spike params `infer_with_cancel_temp` n_ctx 6144 / max_gen 1536).

**Phase D (T064–T075, label [D])** — Option-C ingestion rewrite per ADR-0012: migration 024 `provision_ref` (T064); RED = synthetic real-instrument fixture replicating the gold's failure shapes (T065), gate unit tests incl. hyphenated provisions + boilerplate anchors (T066), tolerant-parse tests (T067), no-LLM acceptance test `lgd_ingest_gate.rs` with canned model response (T068); GREEN = segmentation prompt + tolerant parse, heuristic deleted (T069), `verify_slots` gate (T070), `ingest_template` reshape with model-required refusal FR-010b + retries + progress (T071), `provision_ref` persistence (T072), TemplateManager low-confidence UI (T073); Verify = one batched cargo run + manual real-gold ingest (T074), spike file deletion + CHANGELOG (T075).

**Phase E (T076–T083, label [E])** — semantic two-tier gaps + officer facts: RED = gaps contract tests with injected coverage closure (T076), OfficerSupplied serde + render attribution tests (T077); GREEN = `detect_gaps` pure rewrite with two-tier Gap shape (T078), embedding coverage provider in `gaps_for` with `ECHO_GAP_COVER_THRESHOLD` + whole-corpus tier-a scan (T079), `resolve_gap` acceptSuggestion/officerSupplied (T080), GapPanel/DraftView frontend (T081); Verify = live-corpus threshold calibration ("consultations" → RoB 1986 suggestion; ~10–11 false gaps collapse) (T082), CHANGELOG (T083).

Dependencies updated: D and E independent of Phases B/C and of each other (disjoint backend files; shared `commands/drafting.rs` coordination noted); recommended order **D → E → B → C**. Parallel opportunities listed (RED tasks parallel; frontend [P] alongside backend). Format validated: all 20 tasks follow `- [ ] Txxx [P?] [D|E] description + file path`.

## Outcome

- ✅ Impact: The two live-pilot-blocking defects now have an executable, TDD-ordered task list grounded in actual code locations; both phases independently shippable.
- 🧪 Tests: none run (task generation); 5 RED tasks defined before any GREEN task.
- 📁 Files: specs/009-lgd-drafting/tasks.md (T064–T083 + dependencies + parallel sections).
- 🔁 Next prompts: /sp.implement (Phase D first), or start directly at T064.
- 🧠 Reflection: surveying ingest.rs/gaps.rs/commands before writing tasks let every task name real functions and line anchors — the same discipline that made Phase A tasks executable without re-research.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): checklist-format validation PASS (20/20 tasks well-formed).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): execute T064–T068 (Phase D RED) in one slice.
