---
id: 122
title: Admin-gated sharing batch implementation
stage: green
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [sharing, privacy, ingest, citations, tester-feedback]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src-tauri/src/db/pg/migrations/015_display_name.sql
- src-tauri/src/db/pg/migrations/016_share_requests.sql
- src-tauri/src/db/repositories/share_requests.rs
- src-tauri/src/services/display_title.rs
- src-tauri/src/api/documents.rs
- src-tauri/src/api/admin.rs
- src-tauri/src/api/router.rs
- src-tauri/src/services/indexer.rs
- src-tauri/src/services/query_router.rs
- src-tauri/src/services/structured_importer.rs
- src-tauri/src/services/assistant/capabilities/guidance.rs
- src-tauri/src/db/schema.rs
- src/api/index.ts
- src/pages/SettingsPage.tsx
- src/pages/Admin.tsx
- src/components/ChunkViewer.tsx
- CHANGELOG_DEV.md
tests:
- npx tsc --noEmit (clean)
- cargo check --no-default-features --features server (VM worktree)
---

## Prompt

1,2,3,4,7,8

(Priority selection from the agreed TODO list: 1 = un-share the leaked
compendium copy, 2 = admin-gated sharing, 3 = display-title derivation,
4 = enable ECHO_INSTRUMENT_BOOST, 7 = page-marker hardening, 8 = bare
parent-child citation display.)

## Response snapshot

Ops (live): set the user's compendium copy visibility='private' in prod PG;
added ECHO_INSTRUMENT_BOOST=2.0 to the VM .env, restarted, verified in
/proc/<pid>/environ + health ok.

Code (committed a0ddcb22, pushed to release-deploy — NOT release):
- migration 016 + db/repositories/share_requests.rs; set_visibility now files
  a share request for non-admin owners (never flips visibility); admin
  endpoints list/decide/download; FE Requested state + Admin review queue.
  Column DEFAULT left as 'shared' deliberately (indexer inserts omit it).
- migration 015 + services/display_title.rs (CamelCase/underscore split,
  extension + sort-prefix strip, 6 unit tests); stamped at index time,
  startup backfill, used in retrieval COALESCE sites and as import()'s
  doc_title (fixes title-first classification on CamelCase uploads).
- structured_importer::page_guard — log::error naming the stage that loses a
  form feed (back_matter exempt; re-baselined after it).
- guidance.rs::display_provision_label + ChunkViewer — unresolved chunks show
  the bare ordinal instead of a guessed unit word.

## Outcome

- ✅ Impact: user self-publish removed; citation names/labels improved; page-loss now loud.
- 🧪 Tests: tsc clean; cargo check on VM worktree (in progress at write time).
- 📁 Files: 4 new + 12 modified (see files list).
- 🔁 Next prompts: deploy to release after compile passes; then remaining TODO items 5/6/9/10/11.
- 🧠 Reflection: the visibility DEFAULT trap (indexer inserts rely on it) was caught by reading insert sites before changing the schema.

## Evaluation notes (flywheel)

- Failure modes observed: none yet; compile pending.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
