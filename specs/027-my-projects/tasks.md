---
description: "Task list for Feature 027 — My Projects"
---

# Tasks: My Projects

**Input**: Design documents from `/specs/027-my-projects/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/api.md, quickstart.md
**ADRs**: [ADR-0028](../../history/adr/0028-copy-on-share-and-cross-user-content-linking.md) · [ADR-0029](../../history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md)

**Tests**: **INCLUDED, and not optional here.** The spec's success criteria are stated as
negative tests (SC-001, SC-005, SC-006, SC-007, SC-008), the plan names a *test per route*
as a risk mitigation, and both ADRs name specific SCs as the evidence they must produce.
Isolation that is not tested is isolation that is not delivered.

**Organization**: Grouped by user story. Story phases are ordered by priority **except**
US4 before US5 — copy-on-share can destroy another user's data unless delete-safety lands
first (ADR-0028, plan phasing).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on incomplete work)
- **[Story]**: US1–US7, mapping to spec.md user stories
- Exact file paths in every description

## Path Conventions

Web application (plan.md "Structure Decision"):

- **Server**: `src-tauri/src/` — `api/`, `db/repositories/`, `db/pg/migrations/`, `services/`
- **Server tests**: inline `mod tests` for unit; `src-tauri/tests/*.rs` for integration
  (precedent: `migration_020_test.rs`)
- **Frontend**: `src/` — `pages/`, `components/`, `stores/`, `api/`

⚠️ Cargo manifest is at `src-tauri/`, **not** the repo root. All test commands:
`cargo test --no-default-features --features server` with `NOVIQ_TEST_PG_INSTALL_DIR` set
(quickstart.md). Baseline is **925 passed / 2 failed**; one pre-existing failure
(`sources::scope_tests::conversation_scope_includes_own_attachment`) is in the very module
this feature changes — re-read it before attributing a scope failure to your work.

---

## Phase 1: Setup

**Purpose**: Module scaffolding. No new crates — plan.md confirms the dependency set is unchanged.

- [X] T001 [P] Create empty repository module `src-tauri/src/db/repositories/projects.rs` and register it in `src-tauri/src/db/repositories/mod.rs`
- [X] T002 [P] Create empty repository module `src-tauri/src/db/repositories/chat_shares.rs` and register it in `src-tauri/src/db/repositories/mod.rs`
- [X] T003 [P] Create empty service module `src-tauri/src/services/projects/mod.rs` and register it in `src-tauri/src/services/mod.rs`
- [X] T004 [P] Create empty API module `src-tauri/src/api/projects.rs` and register it in `src-tauri/src/api/mod.rs` (routes registered later, per story)
- [X] T005 [P] Create frontend store `src/stores/projects.ts` with typed shapes matching `contracts/api.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The scoping seam and the schema. Everything else rests on this.

**⚠️ CRITICAL**: No user story work may begin until T014 passes. This phase delivers the
security invariant; it is the one phase where "land it and move on" is not acceptable.

### Schema

- [X] T006 Write migration `src-tauri/src/db/pg/migrations/023_my_projects.sql` creating `project`, `project_source`, `project_chat`, `chat_share` per data-model.md, with a header comment explaining each design choice (`copied_from` is NOT an FK; `project_source` deliberately has NO `owner_id`) and the ⚠️ one-way-deploy warning (`ignore_missing = false`)
- [X] T007 In the same migration, seed `db_config` key `my_projects_enabled` to `'0'` so applying 023 exposes nothing until an administrator turns it on
- [X] T008 [P] Add all five indexes from data-model.md to `023_my_projects.sql`: `idx_project_owner`, `idx_project_source_project`, `idx_project_source_source`, `idx_project_chat_project`, and the partial unique `idx_chat_share_live`
- [X] T009 [P] Write migration test in `src-tauri/tests/migration_023_test.rs` asserting all four tables and five indexes exist, the gate seeds to `'0'`, and `project.copied_from` has no foreign-key constraint (modelled on `migration_020_test.rs`)

### The scoping seam (ADR-0029)

- [X] T010 Add `project_id: Option<String>` and `include_corpus: bool` to `RequestScope` in `src-tauri/src/db/repositories/sources.rs:53`, keeping `Default` and `unrestricted()` at today's behaviour so no existing call site changes meaning
- [X] T011 Add the exclusive-mode branch to `visible_source_ids` in `src-tauri/src/db/repositories/sources.rs:97` — when `project_id` is `Some`, return project memberships only, plus the shared corpus iff `include_corpus`; **must not consult `owner_id`** (ADR-0028), and must leave the `unrestricted()` early-return and the three-branch additive query untouched
- [X] T012 Add doc comment above `visible_source_ids` stating the invariant: this is the only legal place to scope retrieval, per-call-site filtering is forbidden, and why (ADR-0029 Alternative B)
- [X] T013 Implement `owned_project(pool, user_id, project_id) -> Result<Option<Project>>` in `src-tauri/src/db/repositories/projects.rs` — the single ownership check every client-supplied `project_id` must pass

### The negative test — the deliverable of this phase

- [X] T014 Write the isolation test in `src-tauri/src/db/repositories/sources.rs` `mod scope_tests`: given a project holding one source, plus a shared-corpus source, plus another user's source, plus a second project holding a third source, `visible_source_ids` in exclusive mode returns **exactly** the project's own source — asserting each excluded category separately so a failure names which one leaked
- [X] T015 [P] Extend `mod scope_tests` with the corpus case: same fixture with `include_corpus: true` returns the project's source **plus** the shared-corpus source, and still excludes the other user's and the other project's

### The admin gate helper (written now, used by every route as it is added)

- [X] T016 [P] Implement `my_projects_enabled(pool)` and a pure `gate_open(enabled: bool) -> bool` in `src-tauri/src/services/projects/mod.rs`, mirroring the config-read/decision split of `services/deep_search/mod.rs:101-109`, with a unit test for the pure function
- [X] T017 [P] Implement a `require_projects_enabled` guard helper in `src-tauri/src/api/projects.rs` returning the standard `ApiError` when closed, so every route added later calls it as its first act (FR-038)

**Checkpoint**: T014 passes ⇒ the invariant is provable. User story work may begin.

---

## Phase 3: User Story 1 — Ask questions against only my own documents (P1) 🎯 MVP

**Goal**: A project holds documents; questions asked in it reach those documents and nothing else.

**Independent Test**: Create a project, upload two documents, ask a question whose answer
also exists in the shared corpus → every citation resolves to an uploaded document. Ask a
question answerable only from the corpus → "nothing to search", not an answer.

### Tests for User Story 1

- [X] T018 [P] [US1] Route test in `src-tauri/tests/api_projects_isolation.rs`: a project question returns zero shared-corpus, zero other-project and zero cross-user citations (SC-001, chat route)
- [X] T019 [P] [US1] Route test in `src-tauri/tests/api_projects_isolation.rs`: `global_search` with `project_id` returns only that project's documents, and **rejects a project the caller does not own with 404** (FR-013)
- [X] T020 [P] [US1] Route test in `src-tauri/tests/api_projects_isolation.rs`: `source_chunks` and `source_file` scoped to a project refuse a source outside it, and refuse a project the caller does not own — same 404 as genuinely absent
- [X] T021 [P] [US1] Route test in `src-tauri/tests/api_projects_isolation.rs`: `get_knowledge_graph` scoped to a project returns a subgraph over that project's sources only
- [X] T022 [P] [US1] Test in `src-tauri/tests/api_projects_isolation.rs`: an empty project reports "nothing to search" and does **not** fall through to the corpus (FR-012). ⚠️ Assert on the response body — refusals return HTTP 200 on this system

### Implementation for User Story 1

- [X] T023 [P] [US1] Implement `Project` struct and CRUD (`create`, `rename`, `delete`, `list_for_owner`, `get`) in `src-tauri/src/db/repositories/projects.rs`
- [X] T024 [P] [US1] Implement membership functions (`add_sources`, `remove_source`, `sources_for_project`, `documents_with_status`) in `src-tauri/src/db/repositories/projects.rs`
- [X] T025 [US1] Implement `create_project`, `rename_project`, `delete_project`, `list_projects`, `get_project` handlers in `src-tauri/src/api/projects.rs` per contracts/api.md, each calling `require_projects_enabled` first, then `owned_project`
- [X] T026 [US1] Implement `add_project_documents` and `remove_project_document` handlers in `src-tauri/src/api/projects.rs`, returning the `already_present` and `content_retained` counts the contract specifies
- [X] T027 [US1] Add `project_id` to the multipart form of `upload_documents` in `src-tauri/src/api/uploads.rs` so a fresh upload lands in the project, still counting against the uploader's allowance (FR-009)
- [X] T028 [US1] Add optional `project_id` to `global_search` in `src-tauri/src/api/documents.rs:864`, calling `owned_project` before building the scope; absent the field, behaviour is byte-identical to today (FR-040)
- [X] T029 [US1] Add optional `project_id` to `source_chunks` in `src-tauri/src/api/documents.rs:754`, same ownership check
- [X] T030 [US1] Add optional `project_id` to `source_file` in `src-tauri/src/api/documents.rs:672`, same ownership check
- [X] T031 [US1] Add optional `project_id` to `get_knowledge_graph` in `src-tauri/src/api/documents.rs:569`, same ownership check
- [X] T032 [US1] Add the empty-project pre-check to the turn path in `src-tauri/src/services/query_router.rs` so it fires **before** retrieval runs, not after an empty result (data-model.md validation rules)
- [X] T033 [US1] Register all US1 routes in `src-tauri/src/api/router.rs`
- [X] T034 [P] [US1] Build `src/pages/ProjectsPage.tsx` — list, create, rename, delete
- [X] T035 [P] [US1] Build `src/pages/ProjectPage.tsx` — document list with pending/ready/failed state (FR-008), add and remove
- [X] T036 [US1] Wire `src/stores/projects.ts` to the new routes via `src/api/index.ts` and add the nav entry in `src/App.tsx`

**Checkpoint**: A project answers from its own documents and nothing else, on all four routes.

---

## Phase 4: User Story 2 — Keep many conversations inside one project (P1)

**Goal**: A project holds many chats; each reopens with its transcript, and all share the project's documents.

**Independent Test**: Hold three conversations in one project, sign out and back in — all
three listed, each replaying its full transcript with citations.

### Tests for User Story 2

- [X] T037 [P] [US2] Test in `src-tauri/tests/api_projects_chats.rs`: a chat started in a project is answered from that project's documents with **no `project_id` in the request body** — proving the server derives it (ADR-0029)
- [X] T038 [P] [US2] Test in `src-tauri/tests/api_projects_chats.rs`: a forged `conversation_id` belonging to another user's project is rejected by the owner check and does not widen scope
- [X] T039 [P] [US2] Test in `src-tauri/tests/api_projects_chats.rs`: chats started outside a project remain outside every project (FR-019), and a project's chat list excludes them

### Implementation for User Story 2

- [X] T040 [P] [US2] Implement `project_chat` functions (`create`, `for_conversation`, `list_for_project`) in `src-tauri/src/db/repositories/projects.rs`
- [X] T041 [US2] Implement `start_project_chat` and `list_project_chats` handlers in `src-tauri/src/api/projects.rs`, writing the `project_chat` row **server-side** — the only place a chat becomes a project chat
- [X] T042 [US2] In `src-tauri/src/api/query.rs:116`, derive the project from `conversation_id` via `for_conversation`, filter on `owner_id == user_id`, and populate `RequestScope.project_id`; **add no project field to the request body** (contracts/api.md)
- [X] T043 [US2] Extend the history reads in `src-tauri/src/api/history.rs` so a project's chats resolve through `query_log` as today, joined to `project_chat` for grouping (FR-020, account-bound not device-bound)
- [X] T044 [US2] Add the chat list and "new chat" affordance to `src/pages/ProjectPage.tsx`, routing into the existing `src/pages/ChatPage.tsx` with project context
- [X] T045 [US2] Register US2 routes in `src-tauri/src/api/router.rs`

**Checkpoint**: 🎯 **MVP complete.** US1 + US2 are a shippable product on their own.

---

## Phase 5: User Story 4 — Upload once, use in several projects (P2)

**Goal**: One document in many projects; removing it from one leaves the others working.

**⚠️ Sequenced before US5 deliberately.** After copy-on-share, a delete path that forgets
`is_referenced()` destroys **another user's** data. This phase must land first (ADR-0028).

**Independent Test**: Add one document to two projects, remove from the first, confirm it
still answers in the second.

### Tests for User Story 4

- [X] T046 [P] [US4] Test in `src-tauri/tests/api_projects_delete_safety.rs`: a document in two projects, removed from one, remains fully retrievable in the other (SC-004)
- [X] T047 [P] [US4] Test in `src-tauri/tests/api_projects_delete_safety.rs`: a document in exactly one project, removed, has its content destroyed — chunks, embeddings and graph rows gone
- [X] T048 [P] [US4] Test in `src-tauri/tests/api_projects_delete_safety.rs`: `is_referenced` returns true for each of `notebook_sources`, `source_link` and `project_source` **independently**, so no single reference type can be forgotten
- [X] T049 [P] [US4] Test in `src-tauri/tests/api_projects_delete_safety.rs`: deleting a whole project leaves documents held by another project intact

### Implementation for User Story 4

- [X] T050 [US4] Implement `is_referenced(pool, source_id) -> Result<bool>` in `src-tauri/src/db/repositories/sources.rs`, checking all three of `notebook_sources`, `source_link` and `project_source`, with a doc comment naming the cross-user hazard
- [X] T051 [US4] Route the full-delete path at `src-tauri/src/api/documents.rs:599-640` through `is_referenced` so content is destroyed only when nothing references it
- [X] T052 [US4] Route the conversation-cleanup delete path at `src-tauri/src/api/documents.rs:1299` through `is_referenced`
- [X] T053 [US4] Make `remove_project_document` (T026) report `content_retained` truthfully from the `is_referenced` result rather than assuming
- [X] T054 [US4] Add the "add existing document" picker to `src/pages/ProjectPage.tsx`, listing documents the caller already owns

**Checkpoint**: Deletion is safe across projects. Sharing may now be built.

---

## Phase 6: User Story 5 — Share a project; the recipient gets their own copy (P2)

**Goal**: Sharing copies. The recipient owns an independent project with the same documents.

**Depends on**: Phase 5 (T050–T052). Do not start before they pass.

**Independent Test**: A shares with B; B's copy is searchable at once; each edits without
the other seeing it; A deletes a document and B's copy still answers from it.

### Tests for User Story 5

- [X] T055 [P] [US5] Test in `src-tauri/tests/api_projects_sharing.rs`: after sharing, B has their own project with the same sources, searchable with **no reprocessing** and no new `sources` rows created (SC-003)
- [X] T056 [P] [US5] Test in `src-tauri/tests/api_projects_sharing.rs`: changes to either side — add, remove, rename, delete — are invisible to the other, in **both** directions (SC-005)
- [X] T057 [P] [US5] Test in `src-tauri/tests/api_projects_sharing.rs`: A deletes a document, then deletes the whole original project; B's copy still answers from that document (FR-024 — the counter-intuitive one, write it first)
- [X] T058 [P] [US5] Test in `src-tauri/tests/api_projects_sharing.rs`: a question in B's copy retrieves only from B's copy, never the original or a third copy (FR-025)
- [X] T059 [P] [US5] Test in `src-tauri/tests/api_projects_sharing.rs`: sharing does not copy chats — B's copy starts with an empty conversation list (FR-023)
- [X] T060 [P] [US5] Test in `src-tauri/tests/api_projects_sharing.rs`: `usage_for_owner` for B is unchanged by receiving a copy (FR-009)

### Implementation for User Story 5

- [X] T061 [US5] Implement `copy_project(pool, source_project_id, recipient_id)` in `src-tauri/src/services/projects/mod.rs` — one `project` row (recipient as owner, `copied_from` set) plus N `project_source` rows in a transaction; **zero** `sources`, chunks, embeddings or files
- [X] T062 [US5] Implement the `share_project` handler in `src-tauri/src/api/projects.rs`, resolving the recipient by email against `users`, and returning `new_project_id` so the caller sees a copy was made rather than access granted
- [X] T063 [US5] Add the share affordance to `src/pages/ProjectPage.tsx` with the FR-024 disclosure at the point of sharing — stating plainly that shared documents cannot afterwards be recalled from the copy
- [X] T064 [US5] Register US5 routes in `src-tauri/src/api/router.rs`

**Checkpoint**: Sharing works and copies are provably independent.

---

## Phase 7: User Story 3 — Include the law in one question (P2)

**Goal**: A per-question tick that widens retrieval to the shared corpus, then resets.

**Independent of Phases 5–6** — may be built before them if preferred.

**Independent Test**: Ask a corpus-only question unticked → not found. Tick → answered
with labelled corpus citations. Ask again → tick is off by itself.

### Tests for User Story 3

- [X] T065 [P] [US3] Test in `src-tauri/tests/api_projects_corpus_toggle.rs`: with `include_corpus: false`, zero shared-corpus passages are retrieved or cited
- [X] T066 [P] [US3] Test in `src-tauri/tests/api_projects_corpus_toggle.rs`: with `include_corpus: true`, corpus passages are retrieved, and **no other project's or user's** documents are (FR-015)
- [X] T067 [P] [US3] Test in `src-tauri/tests/api_projects_corpus_toggle.rs`: a turn following a `true` turn carries no corpus material forward when sent `false` (FR-018) — neither retrieved passages nor conversation history standing in for them
- [X] T068 [P] [US3] Test in `src-tauri/tests/api_projects_corpus_toggle.rs`: every citation in a mixed answer is labelled as project document or shared-corpus law (SC-011, FR-016)

### Implementation for User Story 3

- [X] T069 [US3] Add `include_corpus: bool` to the query request struct in `src-tauri/src/api/query.rs` alongside `deep`, `widen` and `web`, and thread it into `RequestScope`; **add no column anywhere** — absence of storage is what enforces the reset (ADR-0029 Alternative F)
- [X] T070 [US3] Extend the citation payload in `src-tauri/src/services/query_router.rs` with a provenance marker distinguishing project documents from shared-corpus documents (FR-016)
- [X] T071 [US3] Persist the provenance marker into `citations_json` on the `query_log` turn record so a reopened transcript still shows which answers drew on the corpus (FR-019)
- [X] T072 [P] [US3] Build `src/components/CorpusToggle.tsx` — beside the message box, defaulting to off on every turn, never persisted
- [X] T073 [P] [US3] Render the provenance label on citations in `src/components/AnswerBody.tsx` so an uploaded draft is never mistaken for enacted law

**Checkpoint**: The corpus can be included per question and cannot silently persist.

---

## Phase 8: User Story 6 — Share a chat by link (P3)

**Goal**: A link that signed-in Noviq users can read, with citations, and fork to continue.

**Depends on**: Phase 6 (T061) for the fork.

**Independent Test**: Open the link as another signed-in user → readable, citations open,
no way to ask. Signed out → refused. Fork → an independent project and chat.

### Tests for User Story 6

- [X] T074 [P] [US6] Test in `src-tauri/tests/api_chat_shares.rs`: opening a share link while **not signed in** is refused and leaks nothing of the chat in the body (SC-006)
- [X] T075 [P] [US6] Test in `src-tauri/tests/api_chat_shares.rs`: a signed-in viewer reads the transcript and opens every cited chunk (FR-028)
- [X] T076 [P] [US6] ⭐ Test in `src-tauri/tests/api_chat_shares.rs`: after viewing a shared chat, the viewer's **`global_search` still does not return the owner's project documents** — proving the share did not widen `visible_source_ids` (ADR-0029 Alternative E). This is the test that catches the back door.
- [X] T077 [P] [US6] Test in `src-tauri/tests/api_chat_shares.rs`: no route accepts a new question in a chat the caller does not own (SC-007)
- [X] T078 [P] [US6] Test in `src-tauri/tests/api_chat_shares.rs`: revocation kills the link for everyone, while a copy already taken keeps working (FR-031)
- [X] T079 [P] [US6] Test in `src-tauri/tests/api_chat_shares.rs`: re-sharing an already-shared chat returns the existing token rather than minting a second live credential

### Implementation for User Story 6

- [X] T080 [P] [US6] Implement `chat_share` functions (`mint`, `resolve_live`, `revoke`) in `src-tauri/src/db/repositories/chat_shares.rs`, with a cryptographically random token of ≥128 bits and soft revocation via `revoked_at`
- [X] T081 [US6] Implement `is_chunk_cited_by(pool, conversation_id, chunk_id)` in `src-tauri/src/db/repositories/chat_shares.rs` — the per-chunk authorisation that replaces `visible_source_ids` for viewers (ADR-0029)
- [X] T082 [US6] Implement `share_chat`, `revoke_chat_share`, `get_shared_chat` and `get_shared_chunk` handlers in `src-tauri/src/api/projects.rs`, all taking `AuthUser` so a signed-out caller is refused before the token is examined
- [X] T083 [US6] Implement `fork_shared_chat` in `src-tauri/src/api/projects.rs` — a project copy via `copy_project` plus a copy of the conversation's `query_log` turns under a new `conversation_id` owned by the viewer
- [X] T084 [P] [US6] Build `src/pages/SharedChatPage.tsx` — read-only transcript, openable citations, a fork action, and no composer
- [X] T085 [US6] Add the share and revoke affordances to `src/pages/ChatPage.tsx`, with the FR-032 disclosure that recipients will be able to read the cited passages
- [X] T086 [US6] Register US6 routes and the `/shared/<token>` client route in `src-tauri/src/api/router.rs` and `src/App.tsx`

**Checkpoint**: Links read, do not widen, and can be revoked.

---

## Phase 9: User Story 7 — Administrator turns the feature off (P3)

**Goal**: One deployment-wide switch, enforced at the server, losing nothing.

**Note**: The gate *helper* landed in Phase 2 (T016–T017) and every route has called it
since. This phase adds the control and proves the enforcement.

### Tests for User Story 7

- [X] T087 [P] [US7] Test in `src-tauri/tests/api_projects_gate.rs`: with the gate off, **every** project route is refused — enumerate them from `contracts/api.md` so a route added later without a guard fails this test
- [X] T088 [P] [US7] Test in `src-tauri/tests/api_projects_gate.rs`: with the gate off, a previously working share link is refused (FR-038)
- [X] T089 [P] [US7] Test in `src-tauri/tests/api_projects_gate.rs`: off then on again returns every project, document, chat and link unchanged (SC-009)
- [X] T090 [P] [US7] Test in `src-tauri/tests/api_projects_gate.rs`: with the gate off, ordinary chat, global search and corpus answers are byte-identical to pre-feature behaviour (FR-040, SC-012)

### Implementation for User Story 7

- [X] T091 [US7] Implement `get_my_projects_enabled` and `set_my_projects_enabled` in `src-tauri/src/api/system.rs`, admin-only, persisted to `db_config` with `ON CONFLICT DO UPDATE` and applied at runtime with no restart (mirroring `get/set_chat_extractive`)
- [X] T092 [US7] Audit every route in `src-tauri/src/api/projects.rs` for the `require_projects_enabled` call as its first act, and add the guard to the shared-chat routes
- [X] T093 [P] [US7] Add the My Projects on/off control to `src/pages/Admin.tsx`
- [X] T094 [US7] Hide the projects nav entry in `src/App.tsx` when the gate is off — **presentation only**; the server refusal in T092 is the enforcement

**Checkpoint**: All seven stories functional.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [X] T095 Suite run on VM 2026-08-05: **933 passed / 2 failed** (baseline 925/2 — delta is exactly the 8 new tests). Same two pre-existing failures: `conversation_scope_includes_own_attachment` (STALE FIXTURE — seeds `sources.conversation_id` but no `source_link` row, i.e. pre-022 behaviour) and `rcb_parliament_articles_with_bracket_openers_all_survive` (parser, unrelated). Plus **migration_023 2/2** and **api_projects 5/5**.
- [X] T096 Appended the F027 entry to `CHANGELOG_DEV.md` (2026-08-05): the subtractive-mode rationale, the server-derived chat path, the ⚠️ cross-cutting delete-safety change (`is_referenced` across three tables), the camelCase fail-open, and the deploy/re-index follow-ups
- [X] T097 `CLAUDE.md` updated **by hand** — two Active Technologies lines (no new crates; migration 023 tables) and a Recent Changes entry leading with the exclusive-mode seam, the delete-safety consequence and the `rename_all` + `Option<T>` trap. ⚠️ `update-agent-context.sh` was NOT used: it reports success and writes nothing when the stack dedupes
- [X] T098 Wrote `docs/my-projects-sharing-and-recall.md` — user-facing: a shared document cannot be recalled (FR-024), the table of what a sharer's delete does and does not reach, why chat sharing IS revocable by contrast, and the per-question corpus tick that testers read as a bug
- [X] T099 **Measured on the VM**, new test `copying_a_fifty_document_project_is_metadata_only_and_fast` (`tests/api_projects.rs`): copy of a 50-document project = **0.015 s** against the 5 s budget (SC-003); exclusive-mode `visible_source_ids` = **0.6 ms** at 50 documents (SC-010). ⭐ The load-bearing assertions are the row counts, not the clock — `sources` unchanged at 50, `chunks` **0**, `project_source` exactly doubled to 100 — because a fast machine would pass the timing even on a duplicating implementation given 10-byte fixtures. `api_projects` now **6/6**. ⚠️ The *end-to-end answer* p95 half of SC-010 is NOT measured: it needs the LLM on a deployed instance
- [ ] T100 **Deployed 2026-08-05 21:18 PKT at `bbcd2970` with the gate CLOSED** — so this is no longer blocked on a deploy, it is waiting on an admin enabling the feature. Verified live on prod: migration 023 applied (`success = t`), the four tables exist, `my_projects_enabled = '0'`, an **authenticated** `list_projects`/`create_project` returns **404 "My Projects is not available on this deployment"** (the server refusal, not a hidden nav item), and `global_search` still returns real corpus hits (FR-040). ▶ Remaining: enable the gate, then walk the eight-step manual smoke test with real PDFs and two accounts
- [X] T101 ADR-0028 and ADR-0029 promoted **Proposed → Accepted**, each with a per-clause Evaluator Evidence block naming the test that proves it — including the sabotage check on `project_scope_excludes_everything_but_the_project` and the vacuous-pass incident, and stating plainly what is still unevidenced

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 (Setup)**: no dependencies
- **Phase 2 (Foundational)**: blocks everything. **T014 is the gate** — the invariant must be provable before anything rests on it
- **Phase 3 (US1)** → **Phase 4 (US2)**: US2 needs US1's project and membership repositories
- **Phase 5 (US4)** → **Phase 6 (US5)**: ⚠️ **hard dependency**. Copy-on-share makes deletion cross-user; `is_referenced` must exist first or sharing destroys other users' data
- **Phase 7 (US3)**: independent — may run any time after Phase 4
- **Phase 8 (US6)**: needs Phase 6 (T061 `copy_project`) for the fork
- **Phase 9 (US7)**: needs the routes that exist by then; the guard helper is already in place from Phase 2
- **Phase 10**: after all desired stories

### Story dependency graph

```
Setup → Foundational (T014 ✅)
            ├─→ US1 (P1) ──→ US2 (P1)  🎯 MVP ships here
            │                  ├─→ US3 (P2)  [independent]
            │                  └─→ US4 (P2) ──→ US5 (P2) ──→ US6 (P3)
            └─→ ................................................ US7 (P3)
```

### Parallel opportunities

- **Phase 1**: T001–T005 all parallel
- **Phase 2**: T008, T009 parallel; T015, T016, T017 parallel after T011
- **Phase 3**: all five tests T018–T022 parallel; T023/T024 parallel; T034/T035 parallel
- **Phase 5–9**: every test block within a story is fully parallel
- **Across stories**: US3 can be built by a second developer alongside US4/US5
- ⚠️ **Not parallel**: T028–T031 all edit `src-tauri/src/api/documents.rs`. Same file — sequence them.

## Parallel Example: User Story 1

```bash
# All US1 tests together (they will fail until T023-T033 land):
Task: "Route test — chat isolation in src-tauri/tests/api_projects_isolation.rs"
Task: "Route test — global_search isolation + 404 on unowned project"
Task: "Route test — source_chunks and source_file isolation"
Task: "Route test — get_knowledge_graph isolation"
Task: "Test — empty project reports nothing to search"

# Then the two repository layers together:
Task: "Project CRUD in src-tauri/src/db/repositories/projects.rs"
Task: "Membership functions in src-tauri/src/db/repositories/projects.rs"
```

## Implementation Strategy

### MVP first

1. Phase 1 (Setup) → Phase 2 (Foundational) — **stop at T014 and confirm it fails before T011, passes after**
2. Phase 3 (US1) → Phase 4 (US2)
3. **STOP AND VALIDATE**: a project answers from its own documents on all four routes, and holds many chats
4. Deploy behind the gate (it is off by default) and demo

### Incremental delivery

| Increment | Stories | Ships |
|---|---|---|
| 1 | US1 + US2 | 🎯 MVP — private workspaces with many chats |
| 2 | US4 | Reuse across projects, deletion made safe |
| 3 | US5 | Project sharing |
| 4 | US3 | The law, one question at a time |
| 5 | US6 | Chat links |
| 6 | US7 | Admin control (gate already enforced throughout) |

### Notes

- ⚠️ Write **T057** (A deletes; B's copy still answers) before implementing sharing. It is
  counter-intuitive, it is what FR-024 demands, and it is the behaviour an implementation
  will get wrong.
- ⚠️ Write **T076** (a share viewer still cannot global-search the owner's project) before
  implementing share links. It is the only test that catches the back door.
- ⚠️ Refusals return HTTP 200 on this system. Assert on bodies, not status codes.
- ⚠️ Migration 023 is a one-way deploy — a pre-023 binary fails at startup with
  `VersionMissing` once it has applied.
- Commit after each task or logical group. Stop at any checkpoint to validate.
