# Phase 1 Data Model: My Projects (F027)

**Migration**: `src-tauri/src/db/pg/migrations/023_my_projects.sql` (additive)
**Date**: 2026-08-05

---

## New tables

### `project`

A named container owned by exactly one user. Sharing never grants access to a row here;
it inserts a second row owned by the recipient (FR-020).

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PK | UUID |
| `owner_id` | TEXT NOT NULL | → `users(id)`. Exactly one owner, always (FR-002). |
| `name` | TEXT NOT NULL | User-chosen (FR-001) |
| `created_at` | BIGINT NOT NULL | epoch ms |
| `updated_at` | BIGINT NOT NULL | last activity; drives the FR-004 ordering |
| `copied_from` | TEXT NULL | provenance only — the project this was copied from. **Carries no live relationship**: never read by any scoping or deletion query, and may point at a deleted row. |

- Index `idx_project_owner ON project(owner_id, updated_at DESC)` — the FR-004 listing.
- `copied_from` is deliberately *not* a foreign key. An FK would imply the copy depends
  on its original, which FR-021 says it must not, and `ON DELETE CASCADE` on it would
  destroy the recipient's project when the sharer deletes theirs — the exact failure
  FR-024 forbids.

### `project_source`

Membership. Many-to-many, and **cross-user**: a row may name a project owned by B and a
source owned by A (R4).

| Column | Type | Notes |
|---|---|---|
| `project_id` | TEXT NOT NULL | → `project(id)` ON DELETE CASCADE |
| `source_id` | TEXT NOT NULL | → `sources(id)` ON DELETE CASCADE |
| `added_at` | BIGINT NOT NULL | |
| PK | `(project_id, source_id)` | makes re-adding idempotent — the duplicate-upload edge case |

- Index `idx_project_source_project ON project_source(project_id)` — the retrieval
  lookup, and the hot path for every project question.
- Index `idx_project_source_source ON project_source(source_id)` — the delete-safety
  lookup ("does any project still hold this?").
- ⚠️ There is deliberately **no `owner_id`** on this table, unlike `source_link`. On
  `source_link`, `owner_id` is what stops a guessed conversation id reaching another
  user's attachments. Here the guard is one level up: `project.owner_id` is checked
  before any scope is built (R2), and membership alone then decides. Adding `owner_id`
  here would be actively wrong — it would have to be the *source's* owner, which for a
  copied project is a different user, and any predicate matching it against the caller
  would break exactly the sharing this feature exists to provide.

### `project_chat`

The mapping the system had nowhere to put: `conversation_id` is a client-minted string
with no owning row anywhere (R2).

| Column | Type | Notes |
|---|---|---|
| `conversation_id` | TEXT PK | the same client-minted id `query_log` records |
| `project_id` | TEXT NOT NULL | → `project(id)` ON DELETE CASCADE (FR-036: deleting a project deletes its chats) |
| `owner_id` | TEXT NOT NULL | denormalised from `project.owner_id`, so the chat-turn ownership check is one index lookup with no join |
| `created_at` | BIGINT NOT NULL | |

- Index `idx_project_chat_project ON project_chat(project_id, created_at DESC)` — the
  FR-033 chat listing.
- This row is written **server-side** when a chat is started in a project. It is what
  makes the chat path unwidenable: the client sends only `conversation_id`, and the
  server derives the project from it.

### `chat_share`

| Column | Type | Notes |
|---|---|---|
| `token` | TEXT PK | random, ≥128 bits of entropy — it is the whole credential |
| `conversation_id` | TEXT NOT NULL | the shared chat |
| `owner_id` | TEXT NOT NULL | who shared it; the only user who may revoke |
| `created_at` | BIGINT NOT NULL | |
| `revoked_at` | BIGINT NULL | NULL = live. Revocation is a **soft** update (FR-031), matching how chat deletion already works. |

- Unique partial index `idx_chat_share_live ON chat_share(conversation_id) WHERE
  revoked_at IS NULL` — one live link per chat, so re-sharing returns the same link
  rather than accumulating credentials that must all be revoked separately.

### `db_config` row (no schema change)

`('my_projects_enabled', '0'|'1')`, defaulting to **off**. Seeded off by the migration so
that applying 023 exposes nothing until an administrator turns it on (FR-037).

---

## Changed structures (no column changes)

### `RequestScope` — `src-tauri/src/db/repositories/sources.rs:53`

```rust
pub struct RequestScope {
    pub user_id: Option<String>,
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,   // NEW — Some = exclusive mode
    pub include_corpus: bool,         // NEW — only read when project_id is Some
}
```

`Default` and `unrestricted()` keep today's behaviour (`project_id: None`,
`include_corpus: false`), so the desktop path and all existing tests are unaffected.

### `visible_source_ids` — same file, line 97

Gains one branch **before** the existing logic:

```
if scope.project_id is Some:
    ids = sources indexed AND not disabled AND id IN (project_source WHERE project_id = $1)
    if scope.include_corpus:
        ids += sources indexed AND not disabled AND visibility='shared' AND conversation_id IS NULL
    return ids            // and NOTHING else — no owner_id, no attachments
```

The `unrestricted()` early-return and the three-branch additive query below it are
untouched.

⚠️ Exclusive mode must be reached **only** after `project.owner_id` has been checked
against the caller. The function itself cannot check it without a second query on every
call, so the invariant lives in one helper (`owned_project`) that every route calls, and
in the tests that prove each route calls it.

---

## Content lifetime — the cross-user delete rule

A `sources` row's content may now be referenced from three places:

| Reference | Introduced | Can it be another user's? |
|---|---|---|
| `notebook_sources` | F001 | no |
| `source_link` | 022 | no — per-user by design |
| `project_source` | **023** | **yes** |

**Rule**: content (chunks, embeddings, graph rows, the file) is destroyed only when
**none** of the three holds a reference. One helper —
`source_repo::is_referenced(pool, source_id) -> bool` — and every delete path
(`api/documents.rs:599-640`, `:1299`) consults it.

This is the single most dangerous change in the feature. Before 023, every reference to a
source belonged to the same user, so "delete my document" could only destroy the deleting
user's data. After 023 it can destroy someone else's, and FR-024 depends on it not doing
so.

---

## Entity relationships

```
users ──1:N──> project ──1:N──> project_chat ──1:1──> conversation_id
                  │                                     (client-minted; also the
                  │                                      key query_log records on)
                  └──M:N──> sources   [project_source]
                              ▲
                              │  cross-user: a project owned by B
                              │  may hold a source owned by A
                              │
users ──1:N──> chat_share ──> conversation_id
```

Copying a project (FR-020) writes: 1 `project` row (recipient as owner, `copied_from` set)
+ N `project_source` rows. **Zero** `sources` rows, zero chunks, zero embeddings, zero
files — which is what makes SC-003 (<5 s) reachable and FR-021 (independence) true by
construction rather than by discipline.

---

## Validation rules

| Rule | Source | Where enforced |
|---|---|---|
| Project name non-empty after trim, ≤200 chars | FR-001 | command handler |
| A caller may only name a project they own | FR-002, FR-013 | `owned_project` helper, every route |
| Exclusive mode ignores `owner_id` entirely | FR-020, R4 | `visible_source_ids` |
| Empty project answers "nothing to search" | FR-012 | `run_turn` pre-check — must fire *before* retrieval, not after an empty result |
| `include_corpus` is per-turn and never persisted | FR-017 | request field only; no column exists to store it, by design |
| Share link requires an authenticated user | FR-027 | `AuthUser` extractor on the route — not a token check |
| A share viewer may read only cited chunks | FR-028, FR-029, R5 | separate read path; `visible_source_ids` not consulted |
| Content destroyed only when unreferenced | FR-007, FR-024 | `is_referenced` helper |
| Every project route refuses when the gate is off | FR-038 | middleware or per-route guard, server-side |

---

## State transitions

**Project**: created → (renamed / documents added or removed)\* → deleted.
No shared or locked state exists — copy-on-share means a project is never
concurrently editable, which removes the entire class of transitions that a
shared-access model would have needed.

**Share link**: none → live → revoked. Terminal; re-sharing mints a new token.
Copies taken while live survive revocation (FR-031), because a copy is an independent
project with no reference back.
