# Phase 1 Contracts: My Projects (F027)

All routes are `POST /api/<name>` returning JSON, registered in
`src-tauri/src/api/router.rs` and handled in a new `src-tauri/src/api/projects.rs`.
This matches the command-named convention the SolidJS `transport.call("<name>", {...})`
helper already uses.

**Every route below takes `AuthUser`** (the extractor), and **every route below is
refused when the gate is off** (FR-038) — including the share-link read, which is the
one a user might hold a URL for. The gate check is the first thing each handler does,
before any argument is examined.

**Error shape** is the existing `ApiError { code, message }`. A project the caller does
not own returns `404 not_found` with the same body as a project that does not exist —
whether it exists is itself information, the rule `source_chunks` already follows at
`api/documents.rs:757-762`.

---

## Projects

### `list_projects` → `[Project]`

Request: `{}`. Returns the caller's projects, `updated_at DESC` (FR-004).

```json
[{ "id": "…", "name": "Tender 2026-14", "document_count": 7,
   "chat_count": 3, "created_at": 0, "updated_at": 0, "is_copy": false }]
```

### `create_project` → `Project`

`{ "name": "Tender 2026-14" }` — trimmed, non-empty, ≤200 chars (FR-001).

### `rename_project` → `Project`

`{ "project_id": "…", "name": "…" }` (FR-001).

### `delete_project` → `{ "deleted": true, "chats_deleted": 3 }`

`{ "project_id": "…" }`. Deletes the project, its memberships and its chats (FR-036).
Documents survive wherever any other project still holds them, **including another
user's copy** (FR-007, FR-024). The count is returned so the UI can confirm honestly
rather than guessing.

### `get_project` → `Project & { documents: [ProjectDocument] }`

`{ "project_id": "…" }`.

```json
{ "source_id": "…", "file_name": "bid.pdf", "display_name": "Bid — Vendor A",
  "status": "pending" | "ready" | "failed", "added_at": 0, "added_by_copy": true }
```

`status` is required by FR-008 — a document that is not yet searchable must be visibly
pending, never silently absent from an answer.

---

## Documents in a project

### `add_project_documents` → `{ "added": 2, "already_present": 1 }`

`{ "project_id": "…", "source_ids": ["…"] }`. Adds documents the caller already owns
(FR-006). Re-adding is idempotent, not an error — the duplicate-upload edge case.

Fresh uploads keep using the existing `upload_documents` multipart route, with
`project_id` added to its form fields; it indexes as it does today and then calls the
same insert. Uploads count against the caller's allowance (FR-009).

### `remove_project_document` → `{ "removed": true, "content_retained": true }`

`{ "project_id": "…", "source_id": "…" }`. Removes the membership only. Content is
destroyed only when no `project_source`, `source_link` or `notebook_sources` row remains
anywhere (FR-007). `content_retained` reports which happened, so the caller is told the
truth rather than assuming.

---

## Sharing

### `share_project` → `{ "shared_with": "…", "new_project_id": "…" }`

`{ "project_id": "…", "recipient": "user@example.com" }`

Creates a **new project owned by the recipient** with the same memberships (FR-020).
Does not copy chats (FR-023). Does not copy files, chunks or embeddings (R6) — which is
why it returns in well under the 5 s of SC-003.

The response is deliberately not "access granted": the caller should see that a copy was
made, because FR-024's consequence — that they can no longer recall those documents —
follows from it.

### `share_chat` → `{ "token": "…", "url": "/shared/<token>" }`

`{ "conversation_id": "…" }`. Idempotent: an existing live link is returned rather than
minting a second credential for the same chat.

### `revoke_chat_share` → `{ "revoked": true }`

`{ "conversation_id": "…" }`. Owner only. Copies already taken keep working (FR-031).

### `get_shared_chat` → `{ chat }`

`{ "token": "…" }`. **Requires `AuthUser`** — a signed-out caller gets `401`, never the
chat (FR-027). Returns the transcript with citations.

Read-only: there is no route by which a viewer can add a turn (FR-029). The absence is
the enforcement — no endpoint accepts a question for a chat the caller does not own.

### `get_shared_chunk` → `{ chunk }`

`{ "token": "…", "chunk_id": "…" }`. Authorized by **"is this chunk cited by a turn of
this live share?"** — not by `visible_source_ids`, which is neither consulted nor
widened (R5). This is what keeps a share link from becoming access to the owner's whole
project through global search.

### `fork_shared_chat` → `{ "project_id": "…", "conversation_id": "…" }`

`{ "token": "…" }`. Gives the viewer their own project (the memberships) and their own
copy of the conversation, and they continue from there (FR-030). The original is
untouched.

---

## Chats in a project

### `list_project_chats` → `[{ conversation_id, title, updated_at, is_shared }]`

`{ "project_id": "…" }` (FR-033).

### `start_project_chat` → `{ "conversation_id": "…" }`

`{ "project_id": "…" }`. Writes the `project_chat` row **server-side**. This is the only
place a chat becomes a project chat, and it is why the client never sends `project_id`
on a turn.

---

## Changed existing routes

### `POST /api/query` — two changes

1. **`include_corpus: bool`** joins `deep`, `widen` and `web` as a per-turn flag
   (`api/query.rs:81-83`), which is exactly the lifetime FR-017 requires: it applies to
   this question and is never persisted. No column stores it, by design.
2. **No new project field.** The server derives the project by looking up `project_chat`
   by the `conversation_id` already in the body, then checks `owner_id` (R2). A forged
   conversation id finds no row, or another user's row, which the owner check rejects.

Scope construction at `api/query.rs:116` becomes:

```rust
let chat = project_chat_repo::for_conversation(&pool, &conversation_id).await?;
let project_id = chat.filter(|c| c.owner_id == user_id).map(|c| c.project_id);
let scope = RequestScope {
    user_id: Some(user_id.clone()),
    conversation_id: Some(conversation_id.clone()),
    project_id,                                   // Some ⇒ exclusive
    include_corpus: project_id.is_some() && req.include_corpus,
};
```

### `global_search`, `source_chunks`, `source_file`, `get_knowledge_graph`

Each gains an optional `project_id`. Unlike the chat turn these cannot derive it, so each
**must** call `owned_project(pool, user.id, project_id)` first and return `404` on
failure. Absent the field, behaviour is exactly today's (FR-040).

This is the one place a client names a project directly, and therefore the one place the
"client cannot widen" invariant (FR-013) is enforced by a check rather than by
construction. It needs a test per route, not one shared test.

⚠️⚠️ **Wire keys here are camelCase, and getting that wrong FAILS OPEN.** These four
requests deserialize into structs carrying `#[serde(rename_all = "camelCase")]`, so the
field on the wire is **`projectId`** — and `source_chunks` likewise takes **`sourceId`**,
not `source_id` (pre-existing; sending `source_id` gives 422). This paragraph originally
documented `project_id`, and that mismatch shipped as a real fail-open: an unknown key
against `Option<String>` does not error, it yields `None`, which takes the **unscoped**
branch and answers `200` with the caller's entire corpus where `404` was required.
Fixed at `be522beb` by accepting both — `#[serde(default, alias = "project_id")]` — so
either spelling now works. ⭐ The general rule this earns: **`rename_all` + `Option<T>`
is a fail-open shape in any security-relevant field**, and a scope field should be
alias-tolerant or non-optional, never silently defaulted. It was found by the negative
per-route test above; the positive assertions had been passing vacuously.

---

## Admin

### `get_my_projects_enabled` → `bool` · `set_my_projects_enabled` → `bool`

`{ "enabled": true }`, admin only, persisted to `db_config` and applied at runtime with
no restart — mirroring `get/set_chat_extractive` (`api/system.rs:70-105`) and the
deep-search gate (`services/deep_search/mod.rs:101-109`).

Defaults to **off**, so applying migration 023 exposes nothing until an administrator
turns it on.
