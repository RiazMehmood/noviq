# Research — 021 User Documents

Phase 0 output. All Technical Context unknowns resolved; decisions below are numbered and
referenced from plan.md / data-model.md.

## D1 — Scope truth lives on `sources`, not folders

**Decision**: Add `owner_id TEXT NULL`, `visibility TEXT NOT NULL DEFAULT 'shared'`
(CHECK `IN ('private','shared')`), `conversation_id TEXT NULL` to `sources`.
`indexed_folders` gains `visibility` too, but only as the UI/default value; the columns on
`sources` are what retrieval reads.

**Rationale**: Every retrieval query already joins `sources`
(`query_router.rs:1752/1769/1805/1825`, `analysis_engine.rs:168`); a filter there is one
indexed predicate. Folders are reachable only through the `notebook_sources` →
`__folder__<id>` stub-notebook indirection — pushing the privacy rule through that join in
every retrieval path would spread the invariant across many queries.

**Alternatives considered**: (a) Normalized query-time join through `indexed_folders` —
rejected (3-table join in 4+ paths, invariant smeared across SQL). (b) Keep abusing
`owner_id NULL = shared` with no visibility column — rejected: conflates "admin-owned" with
"shared", can't represent an admin's *private* doc or a user's *shared* doc (US3 needs both).

## D2 — Scope injection point: one id-set function, existing seam

**Decision**: New `visible_source_ids(pool, scope: &RequestScope) -> Vec<String>` in
`db/repositories/sources.rs`, where `RequestScope { user_id: Option<String>, conversation_id: Option<String> }`.
SQL: `visibility='shared' AND conversation_id IS NULL` ∪ `owner_id = $user AND conversation_id IS NULL`
∪ `conversation_id = $conv`. `run_turn` (assistant/mod.rs:341) replaces
`all_indexed_source_ids(pool)` with this call; the set flows through the **existing**
`scope_source_ids` parameters of `get_context_chunks` / `fts5_retrieve` / decompose /
capability paths. Desktop (Tauri commands, no auth) passes `RequestScope::unrestricted()`
→ behaviour identical to today.

**Rationale**: Smallest viable diff — zero retrieval-SQL changes; the privacy rule lives in
exactly one function (auditable, testable). `api/query.rs` already has `AuthUser` and
`conversation_id` in hand at the call site.

**Alternatives considered**: (a) WHERE-clause pushdown into every retrieval query —
rejected: touches ~8 SQL sites, easy to miss one (citation fast-path history shows bypass
paths are the failure mode). (b) Row-level security in PG — rejected: sqlx runtime queries
run as one DB role; per-request role switching is a much bigger change than an id-set.

**Risk noted**: `scope_source_ids = None` currently means "whole notebook" in
`route_scoped`. After this change the web path ALWAYS passes `Some(set)`; tasks must grep
for every retrieval entry point (chat, decompose, drafting, analysis_engine, guidance) and
assert it receives the set — that is SC-001's test surface.

## D3 — Visibility semantics + backfill (FR-015, no re-index)

**Decision**: Migration 009 backfill:
- `sources.owner_id` ← owning folder's `owner_id` (via `notebook_sources.notebook_id = '__folder__'||f.id`), NULL for global.
- `sources.visibility` ← `'private'` where the folder has an owner, else `'shared'`.
- `indexed_folders.visibility` ← same rule.
- Chunks/embeddings untouched → no re-index (FR-015 satisfied by construction).

Folder toggle = one UPDATE on the folder + one bulk UPDATE on its sources (single
transaction). Per-document toggle = UPDATE one `sources` row. Takes effect on the next
query because the scope set is computed per turn (SC-004 ≤ 10 s trivially met).

## D4 — Attachments are sources with `conversation_id`, via the existing pipeline

**Decision**: `POST /api/attach_document` (multipart: file + `conversationId`) saves under
`uploads/<user>/__conv__<id>/`, then `index_sources` into stub notebook
`__conv__<conversation_id>` with `owner_id = user, visibility = 'private',
conversation_id = <id>`. Listings exclude attachments with `conversation_id IS NULL`
filters (FR-017). Promotion = clear `conversation_id`, re-link the source row to the
user's uploads folder stub notebook, move the file — chunks/embeddings untouched (no
re-index). In-conversation scope contains the source either way, so no double-retrieval
(the id set is a set).

**Conversation deletion**: the web UI's delete-conversation action calls a new
`delete_conversation` endpoint that removes that conversation's attachment sources
(chunks/embeddings/files) using the existing FK-safe deletion helpers
(`chunks.rs` delete cascade). Backstop for abandoned conversations: attachments count
against the owner's allowance (FR-019), so they self-limit; a scheduled sweep is OUT of
scope for v1 (noted as follow-up).

**Alternatives considered**: (a) inline the file text into the prompt — rejected: blows the
context budget at ~13 tok/s, no citations, no multi-page support. (b) per-conversation
"hidden folder" rows in `indexed_folders` — rejected: pollutes folder listings/management
paths for no gain; the stub notebook already provides grouping.

## D5 — Per-user allowance

**Decision**: Global limits via env with defaults (`NOVIQ_USER_QUOTA_MB` = 500,
`NOVIQ_USER_QUOTA_FILES` = 200), read at enforcement time in upload/attach handlers.
Usage = `SELECT COALESCE(SUM(file_size_bytes),0), COUNT(*) FROM sources WHERE owner_id = $1`
(includes live attachments per FR-019). Refusal returns usage + limit in the error body.
`GET /api/my_usage` for the user's meter; `GET /api/admin/usage` aggregates per user.

**Rationale**: No new table; `file_size_bytes` already exists on `sources`. Admin
configurability via `.env` matches every other operational knob in this deployment
(`ECHO_HYBRID`, `NOVIQ_INFER_ENGINE`, …). A settings table / per-user overrides are out of
scope per spec assumptions.

## D6 — Constitution §IV divergence (PG) is inherited, not new

**Decision**: Follow the established pattern: additive PG migration
`src-tauri/src/db/pg/migrations/009_sources_ownership.sql` (next after 008_answer_feedback)
plus a mirrored SQLite migration for the desktop build. The §IV amendment remains owed by
F011; this feature adds no new divergence. Recorded in plan.md Complexity Tracking.

## D7 — Which answer surfaces exist on the web (FR-001 audit)

**Finding**: The web router exposes exactly one answer-producing route: `POST /api/query`
(alias `/send_message`) → `run_turn`. There is no web search endpoint (SearchPage is
desktop-scoped); drafting/one-ask commands are desktop-only (Tauri commands, single
operator). Therefore scoping `run_turn`'s id set covers all multi-user surfaces, PROVIDED
every capability inside `run_turn` (guidance, decompose sub-queries, compare/summarize
map-reduce, analysis_engine calls it makes) consumes the per-turn set — that audit is a
dedicated task. Entity/graph read endpoints (`query_entities`, `get_knowledge_graph`, …)
leak only document *structure*, not content, but will get the same source-id filter for
consistency.

**Re-check trigger**: if a future feature web-exposes drafting or search, it must consume
`RequestScope` — add a note to those specs' constitution gates.
