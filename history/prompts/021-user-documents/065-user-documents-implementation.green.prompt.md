---
id: 065
title: User documents implementation wave one
stage: green
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: 021-user-documents
user: Riaz Mehmood
command: /sp.implement
labels: [privacy, retrieval-scoping, uploads, sharing, attachments, migration]
links:
  spec: specs/021-user-documents/spec.md
  ticket: none
  adr: none
  pr: none
files:
- src-tauri/src/db/pg/migrations/009_sources_ownership.sql
- src-tauri/src/db/repositories/sources.rs
- src-tauri/src/db/repositories/folders.rs
- src-tauri/src/db/repositories/graph.rs
- src-tauri/src/services/indexer.rs
- src-tauri/src/services/indexing.rs
- src-tauri/src/services/assistant/mod.rs
- src-tauri/src/services/assistant/capabilities/guidance.rs
- src-tauri/src/services/assistant/capabilities/amendment_chain.rs
- src-tauri/src/services/query_router.rs
- src-tauri/src/api/query.rs
- src-tauri/src/api/uploads.rs
- src-tauri/src/api/documents.rs
- src-tauri/src/api/admin.rs
- src-tauri/src/api/router.rs
- src-tauri/src/commands/assistant.rs
- src/api/index.ts
- src/api/transport.ts
- src/pages/SettingsPage.tsx
- src/pages/ChatPage.tsx
- src/pages/Admin.tsx
- src/components/DocumentTree.tsx
- CHANGELOG_DEV.md
tests:
- tsc --noEmit exit 0
- 6 new unit tests (sources.rs scope_tests) — run on VM
- VM cargo check --features llm,server,cuda (first run caught one syntax error, fixed 67384c18; recheck in flight)
---

## Prompt

run the implementation

## Response snapshot

Implemented T001–T028 (all four user stories) on branch `021-user-documents`
(commits `7841c544` + fix `67384c18`, pushed):

- Foundational: migration 009 (owner_id/visibility/conversation_id on sources +
  folder visibility, backfill, indexes — desktop uses the SAME PG migrator, so
  T003 SQLite mirror is N/A); RequestScope + visible_source_ids (THE invariant,
  one function) + insert_scoped (I1: attachments forced private) +
  scope_for_notebook + usage_for_owner + 6 unit tests.
- US1: run_turn takes scope; all_indexed_source_ids DELETED; web passes
  user+conversation, desktop unrestricted. Audit fixes: guidance empty-scope
  fail-open → always Some(set); citation fast-path clamped to scope;
  amendment_chain + entity/graph endpoints scoped; listings exclude attachments.
- US2: quota before write (env, 500MB/200 files), my_usage/delete_my_document/
  GET /admin/usage, "My documents" section un-gated for all web users, admin
  usage table.
- US3: set_visibility (folder cascade in one tx), list_for_owner = own OR
  shared, badges in My-documents + DocumentTree.
- US4: attach_document (pre-created owned rows, __conv__ notebook),
  list/promote/delete_conversation, ChatPage paperclip + chips (Keep/remove),
  delete-chat triggers server cleanup.

Deviation noted in changelog: admin upload_documents stays global/shared (corpus
workflow); contracts said private-for-all — kept the existing behaviour.

Remaining: T011/T019/T024/T029 browser verification (quickstart §1–§4), T030–T033
polish + merge to release after user verification.

## Outcome

- ✅ Impact: all four stories implemented end-to-end; privacy invariant lives in
  exactly one function; frontend surfaces for upload/share/attach shipped.
- 🧪 Tests: tsc green; VM compile first pass caught 1 syntax error (fixed);
  scope unit tests await VM cargo test run.
- 📁 Files: 33 files, +2712/-54.
- 🔁 Next prompts: VM recheck green → run scope_tests on VM → apply migration to
  prod copy → user browser walkthrough (quickstart) → merge 021 → release.
- 🧠 Reflection: the pre-existing `scope_source_ids: Option<&[String]>` seam made
  US1 nearly SQL-free; the real risk surfaced exactly where predicted (guidance's
  empty→None fail-open default).

## Evaluation notes (flywheel)

- Failure modes observed: leftover `let s =` in a match arm (E0308 + syntax) —
  caught by VM check, not tsc (Rust-only path).
- Graders run and results (PASS/FAIL): tsc PASS; VM check FAIL→fix→recheck pending.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): add `cargo test --lib sources` to the
  VM check script so scope_tests run in the same pass.
