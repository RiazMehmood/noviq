---
id: 0008
title: LGD drafting tasks with UI
stage: tasks
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.tasks
labels: [tasks, lgd-drafting, ui, noviq]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - specs/009-lgd-drafting/tasks.md
tests:
  - planned: src-tauri/tests/lgd_drafting_acceptance.rs (SC-001/002, FR-007, SC-007)
---

## Prompt

/sp.tasks for Feature 009. Include drafting backend (services/drafting, additive migration template_schema/template_slot/gold_example, Tauri commands per contracts, acceptance test) AND frontend per UI Design.jpeg: use "Noviq" (not Echo AI), Noviq logo (noviq_logo_3.png) at top, 3-column research-station layout + green theme, Index Folder option on first screen, full drafting flow UI. Sequence US1 (citation-grade draft from confirmed set) first as MVP, cited-slot path per ADR-0008. Synthetic fixture until real LGD gold. Build as much UI as feasible; mark deferrable items. User clarified: we ARE Noviq (no "rebrand" framing), just use Noviq; do all feasible UI, remainder next session.

## Response snapshot

Wrote specs/009-lgd-drafting/tasks.md — 43 tasks across 8 phases:
- P1 Setup (T001–T005): drafting module + commands scaffold; Noviq logo asset + name strings + green theme tokens.
- P2 Foundational (T006–T012): additive migration (template/slot/gold tables), drafting types, gold-no-retrieval invariant (SC-007), synthetic fixture, Drafting nav + page/store/api stubs.
- P3 US1 MVP (T013–T021): cited-slot via decompose::resolve_provision_label (ADR-0008), fact slots extractive, prose number-free + tier degrade, block assembly + progress, render_draft + export_draft, DraftView UI. Tests first (T013 acceptance, T014 cited-path unit).
- P4 US2 (T022–T027): template ingestion (reuse extraction + heading_detector), confirm/list/get/delete, TemplateManager UI.
- P5 US3 (T028–T031): whole-corpus candidate proposal + role tagging, start_draft/update_doc_set, intent Q&A + doc-set curation UI.
- P6 US4 (T032–T035): gap detection, resolve_gap, file_watcher reuse + docset-updated event, gap panel UI.
- P7 UI shell (T036–T039): 3-column layout, Index-Folder-first-screen (reuse commands/folders.rs), [DEFER] right-rail polish + layout-faithful export.
- P8 Polish (T040–T043): error/remediation messages, CHANGELOG_DEV entry, quickstart validation, [DEFER] swap fixture for real LGD gold.
Dependencies + parallel opportunities + MVP-first strategy documented.

## Outcome

- ✅ Impact: Executable, dependency-ordered task list; MVP = Setup+Foundational+US1 validated by deterministic acceptance test on a fixture (no live LLM in assertions).
- 🧪 Tests: acceptance + cited-path unit planned (SC-001/002, FR-007, SC-007).
- 📁 Files: specs/009-lgd-drafting/tasks.md
- 🔁 Next prompts: /sp.implement (start at T001) or begin MVP slice manually.
- 🧠 Reflection: UI folded into 009 per user; Index-Folder + Noviq branding placed in Setup/Foundational/P7 so the drafting surface is built directly in the target design.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): task-format check — PASS (all tasks have checkbox+ID+story-label-where-applicable+file path).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement T006 (migration) + T007–T011 then T013/T015 to land the cited-slot MVP path first.
