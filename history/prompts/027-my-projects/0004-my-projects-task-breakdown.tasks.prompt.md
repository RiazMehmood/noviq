---
id: 0004
title: My Projects task breakdown
stage: tasks
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: /sp.tasks
labels: [my-projects, tasks, delete-safety, retrieval-scoping, tdd]
links:
  spec: specs/027-my-projects/spec.md
  ticket: none
  adr: history/adr/0028-copy-on-share-and-cross-user-content-linking.md, history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  pr: none
files:
  - specs/027-my-projects/tasks.md
tests:
  - none run (task-generation stage; no code changed)
  - 44 test tasks specified across 7 stories, each traced to a success criterion
---

## Prompt

/sp.tasks

(No arguments. Run against `specs/027-my-projects/` on branch `027-my-projects`.)

## Response snapshot

Generated `specs/027-my-projects/tasks.md` — **101 tasks** across 10 phases, organised by
user story, every one carrying a checkbox, sequential ID, optional `[P]`, story label
where required, and an exact file path. Format validation: **0 violations**; T001–T101
unique with no gaps.

| Phase | Story | Tasks |
|---|---|---|
| 1 | Setup | 5 |
| 2 | Foundational | 12 |
| 3 | US1 — project-only retrieval (P1) 🎯 | 19 |
| 4 | US2 — many chats (P1) 🎯 | 9 |
| 5 | US4 — upload once, delete safety (P2) | 9 |
| 6 | US5 — copy-on-share (P2) | 10 |
| 7 | US3 — corpus per question (P2) | 9 |
| 8 | US6 — chat share links (P3) | 13 |
| 9 | US7 — admin gate (P3) | 8 |
| 10 | Polish | 7 |

### Tests included, deliberately

The template makes tests optional. They are not optional here, and the reason is written
into the file: the spec's success criteria are *stated as negative tests* (SC-001, 005,
006, 007, 008), the plan names a test-per-route as its first risk mitigation, and both
ADRs name specific SCs as the evidence they must produce. 44 of the 101 tasks are tests,
each traced to the FR or SC it discharges.

### Three orderings that are not priority order, and why

1. **US4 (P2) before US5 (P2) is a hard dependency, not a preference.** Copy-on-share makes
   deletion cross-user: after migration 023, a delete path that forgets `is_referenced()`
   destroys *another user's* data. `is_referenced` (T050) and both delete paths
   (T051–T052) must land before `copy_project` (T061) exists to be called.
2. **The admin gate helper is in Phase 2, but US7 is Phase 9.** Every route calls
   `require_projects_enabled` as its first act *as it is written*; T092 audits rather than
   retrofits. Retrofitting a gate is how a UI toggle ends up being the only enforcement.
3. **Phase 2 stops at a test, not at code.** T014 — the isolation negative test — is
   named as the phase gate: it must fail before T011 and pass after, and no story work
   begins until it does.

### Two tasks flagged to write first within their phase

- **T057** (A deletes a document, then the whole project; B's copy still answers from it).
  Counter-intuitive, required by FR-024, and precisely what an implementation will get
  wrong.
- **T076** (after viewing a shared chat, the viewer's `global_search` still returns nothing
  of the owner's project). The only test that catches the back door ADR-0029 Alternative E
  describes — if a share link widened `visible_source_ids`, the transcript would look
  correct while global search leaked the entire project.

### Carried into the tasks rather than left in the plan

`T097` records by hand that `update-agent-context.sh` reports success and writes nothing
when the stack dedupes against an existing entry — verified during `/sp.plan`, so the
`CLAUDE.md` update is a manual task rather than an assumed side-effect. `T028–T031` are
explicitly marked **not** parallel despite being four separate routes: they all edit
`src-tauri/src/api/documents.rs`.

## Outcome

- ✅ Impact: F027 is ready to implement. Dependency graph, parallel opportunities, and a
  six-increment delivery table; MVP is US1 + US2 and ships behind a gate that defaults off.
- 🧪 Tests: none run — no code changed. 44 test tasks specified, each traced to a
  requirement.
- 📁 Files: `specs/027-my-projects/tasks.md`
- 🔁 Next prompts: `/sp.implement`, starting with Phase 1 → Phase 2 and stopping at T014.
- 🧠 Reflection: The template's default is priority order, and following it here would have
  produced a plausible task list that destroys user data — US5 before US4 means sharing
  exists before the delete path knows about cross-user references. The dependency worth
  encoding was not between stories but between a hazard and its guard.

## Evaluation notes (flywheel)

- Failure modes observed: none. One stray zero-width space introduced and removed before
  finalising. The T026/T092 "duplicate IDs" flagged by a naive grep are cross-references
  inside later task descriptions, not duplicate task IDs — verified by matching on
  line-initial IDs only.
- Graders run and results (PASS/FAIL): format checklist — checkbox on every task **PASS**;
  sequential unique IDs T001–T101 **PASS**; `[P]` only where files differ **PASS**; story
  label on every story-phase task and on no setup/foundational/polish task **PASS**
  (24 unlabelled, all in Phases 1, 2, 10); file path in every task **PASS**.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): run Phase 2 alone and confirm T014 fails before
  T011 lands. A negative test that passes before the code exists is testing nothing, and
  that is the cheapest possible moment to discover it.
