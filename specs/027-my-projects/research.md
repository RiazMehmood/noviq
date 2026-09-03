# Phase 0 Research: My Projects (F027)

**Date**: 2026-08-05 | **Branch**: `027-my-projects`

Every finding below was verified by reading the code, not recalled. File and line
references are to the working tree at `1d5f245b`.

---

## R1. Making `visible_source_ids` subtract

**Decision**: Add `project_id: Option<String>` and `include_corpus: bool` to
`RequestScope`. When `project_id` is `Some`, `visible_source_ids` switches to an
**exclusive** mode: project memberships only, plus the shared corpus if and only if
`include_corpus` is set. The existing three-branch additive query is left untouched and
becomes the `None` path.

**Rationale**: `src-tauri/src/db/repositories/sources.rs:97-121` is the single point
through which chat retrieval, global search, citation opening, file serving and the
knowledge graph all scope. Five call sites were confirmed:

| Call site | Route | Today's scope |
|---|---|---|
| `api/query.rs:116` | chat turn | user + conversation |
| `api/documents.rs:569` | `get_knowledge_graph` | user only |
| `api/documents.rs:672` | `source_file` (citation → PDF) | user only |
| `api/documents.rs:754` | `source_chunks` (citation → passage) | user only |
| `api/documents.rs:864` | `global_search` | user only |
| `commands/assistant.rs:62` | desktop | `unrestricted()` |

Because all six funnel through one function, FR-011 (one scoping decision governs every
route) is satisfied by changing that function and nothing else. This is the property
that made the F021 admin-disable take effect everywhere with no re-index, and it is why
scoping must not be implemented anywhere else.

**Alternatives considered**:

- *A fourth OR'd branch.* Rejected — every existing branch only ADDS. A fourth branch
  cannot express "and nothing else", which is the whole requirement. This is the first
  requirement in the system's history that must subtract.
- *Filtering the returned Vec at each call site.* Rejected — six call sites means six
  chances to forget, and a forgotten one fails open. It also directly contradicts the
  reason the function exists.

**Consequence for FR-013 (client cannot widen)**: exclusive mode narrows only if the
caller genuinely owns the project. Passing another user's `project_id` would otherwise
*widen* into their documents. Ownership MUST be verified before the scope is built —
see R2 for why chat gets this for free and the other routes do not.

---

## R2. Project chat identity — there is no conversation table

**Decision**: A new `project_chat(conversation_id PK, project_id, owner_id, …)` table.
The project side owns the mapping. On a chat turn the client sends `conversation_id` as
it does today and **never sends `project_id`**; the server derives the project from the
conversation.

**Rationale**: The design notes assumed one of `query_sessions`, `chat_message` or
`query_messages` carried conversation identity. None does.

- `query_sessions` / `query_messages` (`001_baseline.sql:133-151`) exist but have **zero
  references anywhere under `src-tauri/src/api/`** — they are the vestigial desktop path.
- `chat_message` / `conversation_summary` (`001_baseline.sql:520-538`) are keyed on
  `draft_session_id` and belong to the F009/F010 drafting feature, not to chat.
- The live web identity is `query_log(user_id, conversation_id, …)`
  (`007_query_log_usage.sql:16-29`, extended by `020_chat_history_sync.sql`), where
  `conversation_id` is a **nullable, client-minted string with no owning row**.

So there is no foreign key to hang `project_id` on, and no row whose ownership the
server could check.

**Why deriving beats passing**: `api/query.rs:116` already builds the scope from
`user.id` (from the auth session) and `conversation_id` (from the request body). If the
server looks up `project_chat` by `conversation_id` and checks `owner_id = user.id`, a
forged or guessed `conversation_id` yields either no row or another user's row — and the
owner check rejects the second. **The client never names a project on a chat turn, so
the chat path cannot be widened at all.** This is the same reasoning that added the
owner predicate to the attachment branch (`sources.rs:74-79`).

**The other routes are not free.** Global search, `source_chunks`, `source_file` and
`get_knowledge_graph` all pass `conversation_id: None` and would need the project named
by the client. Each MUST verify ownership through one shared helper
(`project_repo::owned_project(pool, user_id, project_id)`) before building the scope. A
route that skips it turns citation opening into a way to read any project in the
database — the precise failure `source_chunks` already documents at
`api/documents.rs:745-753`.

**Alternatives considered**:

- *`project_id` column on `query_log`.* Rejected — `query_log` is one row per **turn**,
  so the project would be re-asserted by the client on every turn. That is exactly the
  client-supplied scope FR-013 forbids.
- *Reuse `query_sessions`* (it already has `notebook_id` and a chat-session shape).
  Rejected — reviving a table the server has never written to means backfilling identity
  for every existing conversation, and it drags in the `notebooks` FK (see R3).

---

## R3. `notebooks` / `notebook_sources` are live — do not repurpose

**Decision**: New sibling table `project_source` for membership. Leave `notebooks` and
`notebook_sources` untouched.

**Rationale**: The design notes flagged these as possible F001 leftovers with "exactly
the membership shape needed" (`notebook_id, source_id, added_at`). That shape is the
trap. They are load-bearing:

- `api/uploads.rs:599-635` mints a **stub notebook per conversation** and links every
  attachment into it.
- `api/documents.rs:298` mints one **per indexed folder** (`__folder__<id>`).
- Four delete paths unlink them (`documents.rs:400`, `:618`, `:1232`, `:1300`).
- `query_sessions.notebook_id` FKs to `notebooks` (`001_baseline.sql:135`).

The table already carries two distinct meanings. A project would be a third, and
`notebook_sources` rows would then need a discriminator to say which kind they are —
which is the same class of defect as a marker collapsing into a shared key.

**Alternatives considered**: reuse with a `kind` column (rejected — retrofits a
discriminator onto rows four delete paths already interpret); rename `notebooks` to
`projects` (rejected — not additive, and Constitution "Development Constraints" requires
additive migrations).

---

## R4. Cross-user linking — migration 022 deferred exactly this, pending "a spec of its own"

**Decision**: Proceed. This spec is that spec, and an ADR is required.

**Rationale**: `022_source_links.sql:20-24` states the deferral verbatim:

> Scope is PER-USER by decision (2026-08-04). Two different users uploading the same
> bytes still get their own row and their own chunks: cross-user linking would make one
> user inherit another's extraction and leak the fact that the file was already in the
> system, and that needs a spec + ADR of its own.

The deferred hazard is **implicit** dedup: user B uploads a file, the system silently
links B to A's existing row, and B learns the file was already present. Copy-on-share is
**explicit** — A deliberately hands the project to B — so nothing is leaked that A did
not choose to disclose. The stated reason does not apply, but the decision it recorded
must be superseded on the record, not quietly stepped over.

**Consequence — membership, not ownership, is the authority.** A copy references A's
`sources` row, whose `owner_id` is A. `visible_source_ids`' second branch matches
`owner_id = $1`, which will never match B. Therefore **exclusive mode must not consult
`owner_id` at all** — project membership alone decides. Stated positively: in exclusive
mode a source is visible if and only if it is a member of the named project and is
indexed and not disabled.

**Consequence — FR-009 is satisfied for free.** `usage_for_owner`
(`sources.rs:125-134`) sums `file_size_bytes WHERE owner_id = $1`. A copy adds
memberships, never rows, so a recipient's allowance is untouched with no extra code.

**Consequence — delete safety becomes cross-user.** Content may now be referenced from
three places: `notebook_sources`, `source_link` and the new `project_source`, and the
third can be another user's. The delete paths at `documents.rs:599-640` and `:1299` must
consult all three before destroying chunks. Getting this wrong destroys a *different
user's* data — the strongest reason this is P2 work with a dedicated test, not a
side-effect of the copy feature.

---

## R5. Chat share links must not widen `visible_source_ids`

**Decision**: A share-link viewer is authorized **per citation**, not per document set.
Viewing a shared chat grants: the transcript, and the specific chunks cited in it.
`visible_source_ids` is not consulted and not widened.

**Rationale**: FR-028 requires citations to open; FR-029 requires the viewer to gain
nothing else. If a share link added the chat's sources to the viewer's visible set, the
viewer would immediately be able to global-search the owner's whole project, because
global search scopes through the same function (R1). The link would become project
access by the back door — precisely the reading the architect rejected when the two
answers conflicted.

So the shared-chat read path is a separate authorization: *is this chunk cited by a turn
of this shared, unrevoked chat?* It is narrower than a document and cannot be widened by
any existing route.

**Alternatives considered**: adding the chat's sources to the viewer's scope (rejected —
grants the entire project); refusing citations altogether (rejected — the architect chose
citations open, and an answer whose sources cannot be checked violates the spirit of
Constitution XI).

---

## R6. Copying is metadata-only

**Decision**: Copying a project inserts one `project` row and N `project_source` rows.
No file is copied, no chunk re-embedded, no `sources` row created.

**Rationale**: This is what makes SC-003 (<5s) achievable and what makes FR-021
(independence) true by construction — two projects pointing at the same immutable
content cannot affect each other, because neither owns it. Independence is a property of
the data model here, not of code that must remember to preserve it.

Copying a **chat** (FR-030) additionally copies the `query_log` turns into a new
`conversation_id` owned by the recipient, plus the project copy above.

---

## R7. The admin gate follows the deep-search pattern

**Decision**: `db_config` key `my_projects_enabled`, read server-side on every project
request via a pure gate function, mirroring
`services/deep_search/mod.rs:101-109`.

**Rationale**: That pattern separates the config read from the decision
(`deep_enabled(role, audience_is_all)`), which is what makes the gate unit-testable
without a database. `api/system.rs:88-105` shows the `db_config` read/write shape
(`INSERT … ON CONFLICT(key) DO UPDATE`). Deep search also established the lesson FR-038
encodes: the UI toggle is not the gate, the backend is.

**Alternative considered**: an environment variable. Rejected — requires a restart, and
`db_config` applies at runtime, which FR-039 (off then on again, everything intact)
depends on.

---

## R8. Migration number

**Decision**: `023_my_projects.sql`. Confirmed the highest applied is
`022_source_links.sql`.

⚠️ Migrations run with `ignore_missing = false`, so once 023 applies, a pre-023 binary
fails at startup with `VersionMissing` (`022_source_links.sql:26-30`). Additive at the
schema level is **not** the same as a reversible deploy.

---

## R9. Constitution divergences (pre-existing, inherited not caused)

The constitution is at v1.1.0 (2026-06-02) and predates the product's move to a
client-server deployment. Three of its statements no longer describe the system, and
this feature inherits all three without deepening any of them. Carried into the plan's
Complexity Tracking rather than silently ignored.

| Principle | Status |
|---|---|
| I. Privacy-First, Offline-Always | Already superseded by ADR-0025 (private documents may reach third-party APIs). FR-041 discloses this rather than relying on it being untrue. |
| IV. SQLite as single source of truth | Already superseded by F011 (bundled PostgreSQL + pgvector). CLAUDE.md records the amendment as pending. |
| Tech stack: Tauri desktop | The product is client-server; Tauri code is vestigial. This feature is server + SolidJS only. |

Principles IX–XIII (legal domain, officer-first, citation discipline, amendment chain,
cross-reference graph) are all satisfied — see the plan's Constitution Check.

---

## Unknowns remaining

**None.** Every NEEDS CLARIFICATION from the spec was resolved by the architect in
session (see spec "Resolved Decisions"), and every structural unknown was resolved by
reading the code above.

One judgement call is recorded rather than researched: whether the shared-corpus tick
(FR-014) should be a per-turn field on the existing chat request or a separate call. The
plan takes the former — `include_corpus` alongside the existing `deep`, `widen` and `web`
per-turn flags at `api/query.rs:81-83`, which already have exactly this lifetime.
