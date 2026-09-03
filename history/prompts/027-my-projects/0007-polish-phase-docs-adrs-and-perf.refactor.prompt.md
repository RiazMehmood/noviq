---
id: 0007
title: Polish phase — docs, ADR promotion and the measured copy budget
stage: refactor
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: (freeform) "recall memory and complete remaining work of my project feature"
labels: [f027, polish, adr, changelog, performance, documentation]
links:
  spec: specs/027-my-projects/spec.md
  ticket: n/a
  adr: history/adr/0028-copy-on-share-and-cross-user-content-linking.md, history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  pr: none (branch not pushed)
files:
  - CHANGELOG_DEV.md
  - CLAUDE.md
  - docs/my-projects-sharing-and-recall.md
  - history/adr/0028-copy-on-share-and-cross-user-content-linking.md
  - history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  - specs/027-my-projects/contracts/api.md
  - specs/027-my-projects/tasks.md
  - src-tauri/tests/api_projects.rs
tests:
  - "tests/api_projects.rs -- --ignored: 6 passed / 0 failed (was 5/5; +copying_a_fifty_document_project_is_metadata_only_and_fast)"
  - "T099 measurement: 50-document copy = 0.015 s (budget 5 s); exclusive-mode visible_source_ids = 0.6 ms; sources 50, chunks 0, project_source 100"
---

## Prompt

recall memory and complete remaining work of my project feature

## Response snapshot

Recalled `project_027-my-projects` and `project_my-projects-feature`, which named
six open tasks (T096–T101). Closed five and reported the sixth as blocked rather
than closing it weakly.

- **T096** — `CHANGELOG_DEV.md` entry. Written around the *reason* rather than the
  diff: why project scoping needed a new **mode** rather than a fourth OR'd branch
  (every existing branch adds; a fourth cannot express "and nothing else"), why the
  chat path derives the project server-side from `project_chat` instead of trusting
  the client, and the ⚠️ cross-cutting consequence — **deletion can now destroy
  another user's data** unless every delete path consults `is_referenced` across
  `notebook_sources` + `source_link` + `project_source`. Recorded the
  `rename_all` + `Option<T>` fail-open and the sabotage check, plus the ⚠️ one-way
  nature of migration 023.
- **T097** — `CLAUDE.md` by hand: two Active Technologies lines and a Recent Changes
  entry. `update-agent-context.sh` deliberately not used.
- **T098** — new `docs/my-projects-sharing-and-recall.md`. There is no in-repo
  limitations file (the `.docx` pair lives on the Desktop), so this is a focused
  user-facing note: what a sharer's delete does and does not reach, in a table; why
  chat sharing *is* revocable by contrast; and the per-question corpus tick that
  testers will report as a bug.
- **T099** — **measured, not argued**. Added
  `copying_a_fifty_document_project_is_metadata_only_and_fast` and ran it on the VM:
  copy = **0.015 s** against the 5 s budget, exclusive-mode scoping = **0.6 ms**.
  ⭐ The assertions that carry the result are the row counts — `sources` unchanged at
  50, `chunks` **0**, `project_source` exactly doubled — because the clock alone
  would pass a *duplicating* implementation given ten-byte fixture files.
- **T101** — ADR-0028 and ADR-0029 promoted **Proposed → Accepted**, each clause tied
  to the specific test that evidences it, and each ADR stating plainly what is still
  unevidenced.
- Also corrected `contracts/api.md`, which still specified `project_id` for the four
  scoped routes — the exact mismatch that produced the fail-open.

**T100 left open deliberately.** Its automated half is done and green; its
eight-step manual smoke test needs indexed PDFs, a browser and two accounts on a
running instance, and F027 is not deployed. Deploying it is one-way (migration 023
sets `ignore_missing = false`) and is the architect's decision, not a polish step.

## Outcome

- ✅ Impact: F027 polish phase closed at **100/101 tasks**. All decision records are
  Accepted with named evidence; the delete-safety consequence and the camelCase
  fail-open are now recorded in the three places a future maintainer will actually
  look (`CHANGELOG_DEV.md`, `CLAUDE.md`, the contract).
- 🧪 Tests: `api_projects` **6/6** on the VM (was 5/5). No source code changed, so the
  lib suite is unaffected at 933/2. Migration 023 remains 2/2.
- 📁 Files: 8 changed, committed as `d6340032`. Branch still **not pushed, not
  deployed**; the gate still seeds `'0'`.
- 🔁 Next prompts: decide whether to deploy F027 behind its closed gate (unblocks
  T100); ticket the stale `conversation_scope_includes_own_attachment` fixture, which
  is the only coverage of the attachment branch's owner predicate.
- 🧠 Reflection: the memory file's warning about `rename_all` + `Option<T>` did real
  work here — it pointed at `contracts/api.md` as a place still carrying the wrong
  spelling, which nothing in the task list mentioned.

## Evaluation notes (flywheel)

- Failure modes observed: none new. The one avoided was writing T099 as a pure
  timing assertion, which would have been a green test proving nothing — the same
  shape as the vacuous positive assertions found in the previous session.
- Graders run and results (PASS/FAIL): none. F027's guarantees are structural, not
  answer-quality; the test suite is the evidence.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): deploy behind the closed gate, then run
  only step 1–4 of the quickstart smoke test (isolation) before enabling anything
  broader.
