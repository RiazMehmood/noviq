---
id: 019
title: Web folder-indexing surface for the browser thin client
stage: green
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: recall memory and start working on remaining work and issues
labels: [frontend, folders, thin-client, web-transport]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - src/pages/SettingsPage.tsx
tests:
  - tsc --noEmit (exit 0)
---

## Prompt

recall memory and start working on remaining work and issues.

## Response snapshot

Recalled the F014 deployment memory + `RESUME_FOLDERS_NOT_NOTEBOOKS.md`. The
"index folders, no notebooks" decision left ONE actionable local gap: a browser
user cannot index anything because the only folder-indexing surface
(`SettingsPage.tsx` "Add Folder") calls the Tauri native `open({ directory:true })`
dialog, which is a no-op in the web build.

Fix (smallest diff): branch the Indexed-Folders add control on `isTauri()`.
- Desktop (`isTauri()` true): unchanged native directory dialog → `handleAddFolder`.
- Web (fallback): a text input for an absolute **server** path + "Index Folder"
  button → `handleAddFolderWeb` → `addIndexedFolder(path)` (already mounted at
  `POST /api/add_indexed_folder`). Enter key submits; button disabled while empty
  or indexing. Both paths funnel through a new `indexFolderPath` helper
  (`addIndexedFolder` + `loadFolders`). Per-file progress already streams via the
  existing folder-progress `listen`/SSE, so no progress wiring was needed.

`tsc --noEmit` → exit 0.

## Outcome

- ✅ Impact: browser thin client can now index the central corpus by typing a
  server-side absolute path; closes the primary frontend gap from the resume plan.
- 🧪 Tests: `node node_modules/typescript/bin/tsc --noEmit` → exit 0.
- 📁 Files: src/pages/SettingsPage.tsx (isTauri import; webFolderPath signal;
  handleAddFolder/handleAddFolderWeb/indexFolderPath; transport-branched render).
- 🔁 Next prompts: (optional) deprecate/gate notebook create + `addSources` file
  path in the web build (SourceList.tsx still uses native dialog); optionally
  retire `/api/add_sources` + `/api/list_notebook_sources`. VM-only: T028 latency
  (now unblocked), T021/T048 ignored tests, T051/T052 finalize.
- 🧠 Reflection: the folder server-side path already worked; the only fix was a
  browser-reachable trigger for it.

## Evaluation notes (flywheel)

- Failure modes observed: global `npx tsc` resolves wrong TS version; must invoke
  `node node_modules/typescript/bin/tsc`.
- Graders run and results (PASS/FAIL): tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): gate the notebook source UI in web build.
