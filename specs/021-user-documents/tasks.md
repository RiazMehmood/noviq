# Tasks: User Documents — Ownership-Scoped Retrieval, Uploads, Sharing, and Chat Attachments

**Input**: Design documents from `/specs/021-user-documents/`
**Prerequisites**: plan.md, spec.md, research.md (D1–D7), data-model.md, contracts/api.md, quickstart.md

**Tests**: Spec SC-001 demands a privacy test; unit tests included for the scope function
(the privacy invariant) and quota math. Everything else verifies via quickstart.md
walkthrough + SQL asserts vs live PG (F018 pattern — embedded-PG tests unrunnable on VM glibc 2.31).

**Organization**: grouped by user story; US1 alone is the shippable privacy-fix MVP.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup

- [x] T001 Read `CHANGELOG_DEV.md` top entries + re-read `specs/021-user-documents/{plan,research,data-model}.md`; confirm branch `021-user-documents` is current and rebased on `release`

---

## Phase 2: Foundational (blocking — schema + scope primitive used by every story)

- [x] T002 Write additive PG migration `src-tauri/src/db/pg/migrations/009_sources_ownership.sql`: `sources` +`owner_id TEXT`, +`visibility TEXT NOT NULL DEFAULT 'shared' CHECK (visibility IN ('private','shared'))`, +`conversation_id TEXT`; `indexed_folders` +`visibility` (same check/default); indexes `idx_sources_owner`, `idx_sources_conversation`, partial `idx_sources_shared`; backfill per data-model.md (owner/visibility from owning folder via `notebook_sources.notebook_id = '__folder__'||f.id`; folders: owner NULL→shared else private); register in the PG migration runner
- [x] T003 [P] ~~Mirror the migration for the desktop build in `src-tauri/src/db/migrations/`~~ **N/A — since F011 the desktop build boots the SAME bundled-PG migrator (`schema.rs` `sqlx::migrate!("./src/db/pg/migrations")`); the SQLite migrations dir is legacy dead code. One PG migration serves both builds.**
- [x] T004 Extend `src-tauri/src/db/repositories/sources.rs`: add `owner_id`/`visibility`/`conversation_id` to the `Source` struct + insert path (new optional params, default shared/NULL to keep desktop callers unchanged); add `pub struct RequestScope { user_id: Option<String>, conversation_id: Option<String> }` with `unrestricted()`; add `visible_source_ids(pool, &RequestScope) -> Result<Vec<String>>` implementing exactly: `(visibility='shared' AND conversation_id IS NULL) OR (owner_id=$user AND conversation_id IS NULL) OR (conversation_id=$conv)`; add `usage_for_owner(pool, user_id) -> (bytes, files)`
- [x] T005 Thread ownership into indexing: `src-tauri/src/services/indexing.rs` — `index_sources`/`index_folder` accept `owner_id: Option<String>`, `visibility: &str`, `conversation_id: Option<String>` for NEW source rows (existing callers pass today's values: admin folder → NULL/shared, user upload → owner/private); `src-tauri/src/api/uploads.rs` passes the user's ownership through
- [x] T006 [P] Unit tests for the invariant in `src-tauri/src/db/repositories/sources.rs` `#[cfg(test)]` (SQLite in-memory, F018 style): unrestricted returns all; user scope excludes other users' private + all foreign-conversation attachments; conversation scope includes own attachments; shared visible to everyone; invariant I1 (attachment ⇒ owned+private) enforced by insert path

**Checkpoint**: `cargo check` green on VM; migration applies cleanly to a prod-copy; `SELECT visibility, count(*) FROM sources GROUP BY 1` shows all-shared (corpus unchanged)

---

## Phase 3: US1 — Answers only draw from documents I'm allowed to see (P1) 🎯 MVP

**Goal**: every answer-producing retrieval on the web is restricted to shared ∪ own ∪ active-conversation attachments; desktop unchanged.
**Independent test**: quickstart.md §1 (two-user exclusion + admin exclusion + corpus regression).

- [x] T007 [US1] `src-tauri/src/services/assistant/mod.rs` — `run_turn` takes `scope: &RequestScope`; replace `all_indexed_source_ids(pool)` (line ~341) with `visible_source_ids(pool, scope)`; desktop Tauri command callers pass `RequestScope::unrestricted()`
- [x] T008 [US1] `src-tauri/src/api/query.rs` — build `RequestScope { user_id: Some(user.id), conversation_id: Some(req.conversation_id) }` and pass into `run_turn`
- [x] T009 [US1] Capability-path audit (research D2 risk): verify EVERY retrieval inside `run_turn` consumes the per-turn `scope_ids` and none re-queries unscoped — check `services/assistant/capabilities/*.rs`, `mapreduce.rs`, `intent_router::decompose` sub-queries, `analysis_engine.rs`, guidance chunk-lookup (`capabilities/guidance.rs:81`), and both retry paths in `query_router.rs` (~1393/1400); fix any that call `all_indexed_source_ids` or notebook-wide SQL directly; document each verified path in the PR/changelog entry
- [x] T010 [P] [US1] Scope the read-only structure endpoints for consistency: `src-tauri/src/api/documents.rs` `query_entities`/`get_entity_relationships`/`get_entity_sources`/`get_knowledge_graph` filter to `visible_source_ids` of the requesting user
- [ ] T011 [US1] VM verification: compile (`cargo check` server + llm,server), run migration on prod copy, execute quickstart.md §1 with two test users + admin; record latency delta vs pre-feature on 3 standing corpus questions (SC-002 ≤10%)

**Checkpoint**: US1 shippable on its own — the standing privacy gap is closed even before any UI exists

---

## Phase 4: US2 — My documents: upload and manage my own files (P2)

**Goal**: ordinary web users upload/list/delete their own private documents with allowance + admin oversight.
**Independent test**: quickstart.md §2 (sign-in → cited answer <5 min; quota refusal; delete removes from answers).

- [x] T012 [US2] `src-tauri/src/api/uploads.rs` — enforce allowance BEFORE saving (env `NOVIQ_USER_QUOTA_MB` default 500, `NOVIQ_USER_QUOTA_FILES` default 200; `usage_for_owner`): refuse with `{code:"quota_exceeded", used_bytes, used_files, limit_bytes, limit_files}`; per-file unsupported-type rejection leaves rest of batch processing (FR spec US2-AS4)
- [x] T013 [P] [US2] New endpoints in `src-tauri/src/api/documents.rs`: `my_usage` (POST, AuthUser → usage+limits) and `delete_my_document` (POST `{sourceId}`, owner-or-admin; reuse `delete_folder_source` cascade internals + file removal; audit `delete_document`/`admin_delete_document`)
- [x] T014 [P] [US2] `src-tauri/src/api/admin.rs` — `GET /admin/usage`: per-user `{userId, username, usedBytes, usedFiles}` aggregate over `sources.owner_id`
- [x] T015 [US2] Register routes in `src-tauri/src/api/router.rs` (`/my_usage`, `/delete_my_document`, `/admin/usage`)
- [x] T016 [US2] Frontend transport `src/api/transport.ts`: `myUsage()`, `deleteMyDocument(sourceId)`; surface `quota_exceeded` errors with usage message
- [x] T017 [US2] Frontend "My documents" section in `src/pages/SettingsPage.tsx`, OUTSIDE the `showAdvanced()` gate: for every web user — upload button (reuse existing `uploadDocuments` + SSE progress), own-documents list (status/retry via existing endpoints), delete with confirm, usage meter (used/limit); admin-only folder management stays where it is
- [x] T018 [P] [US2] Admin FE: per-user usage table in `src/pages/Admin.tsx` fed by `/admin/usage`
- [ ] T019 [US2] Verify quickstart.md §2 end-to-end in browser (fresh user, quota env override, admin delete + audit row)

**Checkpoint**: users have private working documents; privacy from US1 keeps them invisible to others

---

## Phase 5: US3 — Share a document with everyone (P3)

**Goal**: explicit Private/Shared toggle, owner/admin only, effective immediately, no re-index.
**Independent test**: quickstart.md §3 (share → B sees+cites; unshare → B loses; 403 for non-owner; audit rows).

- [x] T020 [US3] `src-tauri/src/db/repositories/folders.rs` + `sources.rs`: `set_visibility_folder(pool, folder_id, vis)` (folder row + bulk child-sources UPDATE, one transaction — invariant I3) and `set_visibility_source(pool, source_id, vis)`; add `visibility` + owner username to folder/source list DTOs (join `users` for `owner_username`)
- [x] T021 [US3] `src-tauri/src/api/documents.rs` — `set_visibility` endpoint per contract (exactly one of folderId/sourceId; owner-or-admin via `can_manage_folder`/source owner check; audit `share`/`unshare`); register in `router.rs`
- [x] T022 [US3] Listings include shared docs for everyone: `folders::list_for_owner` returns own + `visibility='shared'` folders (not just `owner_id IS NULL`), and source listings expose visibility/owner; attachments (`conversation_id IS NOT NULL`) excluded from every listing (FR-017)
- [x] T023 [US3] Frontend: visibility badge + toggle in the My-documents section (`src/pages/SettingsPage.tsx`) and Private/Shared + owner badge in `src/components/DocumentTree.tsx`; `setVisibility()` in `src/api/transport.ts`
- [ ] T024 [US3] Verify quickstart.md §3 in browser (flip both directions incl. an admin corpus doc, 10 s propagation, 403 case, audit rows)

**Checkpoint**: team knowledge sharing works; corpus behaviour byte-identical for pre-existing docs

---

## Phase 6: US4 — Attach a file to a conversation (P4)

**Goal**: conversation-scoped attachments through the existing pipeline; promotable; die with the conversation.
**Independent test**: quickstart.md §4 (cited in-conversation ≤2 min; invisible elsewhere; promote; failure retry; delete cleans up).

- [x] T025 [US4] `src-tauri/src/api/uploads.rs` (or new `attachments.rs`) — `attach_document` per contract: multipart `conversationId`+files, allowance check, save under `uploads/<user>/__conv__<id>/`, `index_sources` into notebook `__conv__<conversationId>` with `(owner=user, visibility='private', conversation_id)`; audit `attach`; register route with upload body limit
- [x] T026 [P] [US4] `src-tauri/src/api/documents.rs` — `list_conversation_attachments` (requesting user's attachments for that conversation: id/name/status/failReason) and `promote_attachment` (`{sourceId}`, owner only: clear `conversation_id`, re-link notebook_sources row to user's uploads folder stub notebook — create folder if absent, reusing `uploads.rs` get-or-create logic — move file from `__conv__` dir; NO re-index; audit `promote`)
- [x] T027 [US4] `delete_conversation` endpoint: FK-safe cascade delete of all sources with that `conversation_id` (+ files + `__conv__` notebook row); audit `delete_conversation`; register route
- [x] T028 [US4] Frontend `src/pages/ChatPage.tsx` + `src/api/transport.ts`: paperclip attach in the composer (`attachDocument`), attachment chips above composer (progress via `/index/events` SSE, ready/failed states, retry via existing `retry_folder_source`, remove, Promote button); call `deleteConversation` when the user deletes a chat session; `listConversationAttachments` on conversation open
- [ ] T029 [US4] Verify quickstart.md §4 in browser incl. psql check `SELECT count(*) FROM sources WHERE conversation_id='<C1>'` → 0 after delete

**Checkpoint**: all four user stories live

---

## Phase 7: Polish & Cross-Cutting

- [ ] T030 [P] Run quickstart.md §5 DB spot checks on prod (visibility distribution, invariant I1 = 0 rows, audit coverage SC-006)
- [ ] T031 [P] Dedup guard (spec edge case): verify `file_hash` lookups in the indexing path are per-owner (`owner_id IS NOT DISTINCT FROM`) so identical files under two owners stay independent
- [ ] T032 Append `CHANGELOG_DEV.md` entry (why/fix/files/follow-ups incl. deferred conversation-orphan sweep) and update memory notes
- [ ] T033 Merge `021-user-documents` → `release`, push (auto-deploy), post-deploy health check + repeat quickstart §1 privacy smoke on prod

---

## Dependencies

- Phase 2 blocks everything (schema + `visible_source_ids`).
- **US1 (T007–T011) depends only on Phase 2** → MVP; ship alone if desired.
- US2 (T012–T019) depends on Phase 2; independent of US1 for code, but MUST NOT deploy before US1 (uploads without scoping = live privacy leak).
- US3 (T020–T024) depends on Phase 2 + US2's UI section for the toggle placement (T023 after T017).
- US4 (T025–T029) depends on Phase 2 + US1 (conversation term in the scope SQL) + reuses US2 allowance (T012).
- Polish after all stories.

## Parallel opportunities

- T002 ∥ T003 (different migration dirs); T006 alongside T004 review.
- Within US1: T010 ∥ T009. Within US2: T013 ∥ T014 ∥ T018; T016/T017 after routes.
- US2 backend (T012–T015) can proceed in parallel with US1's T009 audit (different files).

## Implementation strategy

**MVP = Phase 2 + US1** (privacy fix, zero UI) — deployable immediately and valuable alone.
Then US2 → US3 → US4 as independent increments, each ending with its quickstart section in
the browser before moving on. Single deploy at the end is acceptable too (small team), but
US1 must be in any deploy that contains US2+.
