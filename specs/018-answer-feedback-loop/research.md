# Phase 0 Research — Answer Feedback Loop

All unknowns resolved against the live codebase (no NEEDS CLARIFICATION remain).

## D1 — Feedback anchor: `chat_message.id` vs `query_log.id`

- **Decision**: Anchor `answer_feedback.message_id` on **`chat_message.id`**.
- **Rationale**: `run_turn` persists every assistant turn via `memory::append_turn`
  (`services/assistant/memory.rs:293`), which mints a `chat_message` row and returns its id;
  `run_turn` stamps it into `TurnResponse.message_id` (`services/assistant/mod.rs:399`). That id is
  already serialized to the browser in the `/api/query` response body and retained per-turn in
  `ChatPage.tsx` (`resp().message_id`), so the frontend can post feedback with **no extra round-trip
  or schema change to obtain an id**. This exactly matches spec FR-003 ("linked to the exact
  `chat_message`").
- **Alternatives considered**:
  - `query_log.id` — also one row per turn (`api/query.rs:89`), but its UUID is generated *inside*
    `query_log::insert` and **not returned** to the caller or the frontend. Using it would require
    changing the insert signature and threading the id back through `TurnResponse`. More diff, no
    benefit.
  - Synthetic `(conversation_id, seq)` key — brittle; `seq` is not currently exposed to the client.
- **Correlation note**: `query_log` and `chat_message` share `conversation_id`; the admin/SME
  surfaces can JOIN feedback → `chat_message` → `query_log` (same conversation, nearest `created_at`)
  to pull latency/citations/resource-peak for a rated turn. No FK from `answer_feedback` to
  `query_log` is needed.

## D2 — Is the FR-004 "full answer trace" already satisfied?

- **Decision**: **Yes — reuse existing trace capture; build no new trace persistence** in this plan.
- **Rationale**: Per turn the server already persists (a) `query_log` — question, final answer,
  `citations_json`, `latency_ms`, cpu/ram/gpu peak (`db/repositories/query_log.rs`); and (b) on-disk
  query-trace files surfaced by `GET /api/traces/latest|all|recent` and
  `read_latest_query_trace` (`api/router.rs:61-65`), which carry decomposition, retrieved chunk
  IDs+scores, prompts, agent decisions, engine, and budget audit (`TurnResponse.audit`). Together
  these meet FR-004's enumerated trace fields. The feedback row only needs the `message_id` handle;
  diagnosis JOINs to the existing records.
- **Follow-up (not this plan)**: if trace files are rotated/pruned, an SME opening an old rated turn
  may hit "trace incomplete" — spec Edge Case already accounts for this (store rating anyway, flag
  incomplete). A retention policy is a deferred clarification.

## D3 — Re-tap / changed-answer semantics (spec Edge Case: 👍→👎 analog)

- **Decision**: **Latest-write-wins per (message_id, user, question)**, keeping history.
- **Rationale**: Spec: "keep the latest with history, not a duplicate." Implement as INSERT of a new
  row carrying `supersedes = <prior row id>` (or NULL), plus a partial unique index so only the
  newest row per (message_id, user) is "current". Aggregates (FR-009) count only current rows.
- **Alternatives considered**: in-place UPDATE — loses history, blocks any later "did users change
  their mind" analysis; rejected. Hard unique constraint with UPSERT overwrite — same history loss.
- **Simplest viable**: a `superseded_at` timestamp (NULL = current) set on the prior row at write
  time, avoiding a self-referential id chain if we prefer. data-model.md picks the concrete shape.

## D4 — Two independent questions, partial answers

- **Decision**: Both `citation_accuracy` and `citation_completeness` are **nullable**; a feedback row
  is valid with either one set. The UI posts whichever the user tapped (may post twice as they tap).
- **Rationale**: Spec User Story 1 AS-2: "either question may be answered independently." Simplest is
  one row per (message_id, user) holding both nullable columns; tapping the second question updates
  the same current row (via the supersede path) to fill the second column.

## D5 — Unauthenticated / desktop turns (NULL user)

- **Decision**: `user_id` nullable (mirrors `query_log.user_id`, "NULL only for the desktop").
- **Rationale**: The web `/api/feedback` route sits behind the `AuthUser` extractor (real user id);
  the desktop path has no auth and passes NULL, consistent with existing tables. Aggregates group by
  user where present.

## D6 — Enum storage form

- **Decision**: Store **short text codes**, validated by a `CHECK` constraint, not a PG `ENUM` type.
- **Rationale**: The codebase stores such small vocabularies as `TEXT` (e.g. `role`, config values);
  `CHECK (citation_accuracy IN ('all','some','wrong','unsure'))` keeps the migration additive and
  avoids a typed-enum migration dance. Display labels live in the frontend (locked in spec
  Clarifications), decoupled from stored codes.

## Summary of decisions

| # | Decision |
|---|----------|
| D1 | Anchor on `chat_message.id` (already in `TurnResponse.message_id`). |
| D2 | Reuse existing `query_log` + `/api/traces/*` for FR-004; no new trace store. |
| D3 | Latest-write-wins with history (supersede), current-row partial unique index. |
| D4 | Both citation columns nullable; either question answerable alone. |
| D5 | `user_id` nullable (web=real id, desktop=NULL). |
| D6 | `TEXT` + `CHECK` for the two small enums; labels in FE. |
