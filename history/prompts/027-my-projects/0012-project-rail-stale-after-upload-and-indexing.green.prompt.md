---
id: 0012
title: Project rail stale after upload and during indexing
stage: green
date: 2026-08-29
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational)
labels: [f027, projects, ui, reactivity, polling, bugfix]
links:
  spec: specs/027-my-projects/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/components/AddProjectDocuments.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: clean"
  - "vite build: clean (index-BLCDj8Yy.js)"
  - "browser behaviour: reported by the architect, fix NOT yet confirmed live"
---

## Prompt

now th intrface is ok, but during indexing and adding documents, it never shows document we need to refresh the screen,and after that again it is showing indexing only after that it needs to again reload the page to show.

## Response snapshot

Two separate misses, both "nobody re-read the server".

1. `AddProjectDocuments` has two tabs. The "My documents" tab calls
   `addDocuments()`, which refreshes via `openProject()`. The "Upload new" tab
   does not go through it — the server attaches during upload — so nothing put
   the new rows into `documents()`. `onUpload` now awaits `openProject()`.
   ⚠️ Third bug on this same path; two tabs doing one job through different code
   keeps producing them.

2. Indexing is a background job the server pushes nothing about, so the
   "indexing…" pill never changed. New `createEffect` polls `openProject` while
   any document is `pending`, backing off 4s → 20s after a minute.

⭐⭐ The poll must not read `projectDocuments()` in the effect body: refreshing
sets that signal, the effect re-runs, and the interval is torn down and rebuilt
on every refresh. The interval is created once per project and the pending check
lives in the timer callback, which is not a tracked scope.

## Outcome

- ✅ Impact: a document appears the moment it is added and flips to searchable on its own.
- 🧪 Tests: typecheck + build clean; live behaviour not yet confirmed.
- 📁 Files: 2 changed + changelog.
- 🔁 Next prompts: confirm on the live build; consider collapsing the two add
  paths so the next field added does not go stale on one of them.
- 🧠 Reflection: both halves were the same defect wearing two faces — client
  state that is only ever written once, for a server fact that changes later.

## Evaluation notes (flywheel)

- Failure modes observed: I shipped the retirement change without exercising the
  add-document path in a browser, and said so at the time; the architect found
  both defects on first real use.
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): run the app locally before calling a
  UI change done, rather than relying on typecheck plus build.
