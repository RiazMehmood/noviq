# Phase 1 Data Model — Answer Feedback Loop

## Entity: Answer Feedback

One current row per **(message_id, user_id)**; re-taps supersede the prior row (history retained).

### Table `answer_feedback` (PostgreSQL, additive migration `008`)

| Column | Type | Null | Notes |
|--------|------|------|-------|
| `id` | TEXT PK | no | UUID v4 (matches `query_log`/`chat_message` id style). |
| `message_id` | TEXT | no | The rated assistant turn = `chat_message.id` (from `TurnResponse.message_id`). |
| `conversation_id` | TEXT | yes | Denormalized for cheap admin JOIN to `query_log`. |
| `user_id` | TEXT | yes | Web = session user; NULL for desktop (mirrors `query_log`). |
| `citation_accuracy` | TEXT | yes | One of `all` / `some` / `wrong` / `unsure`. NULL until answered. |
| `citation_completeness` | TEXT | yes | One of `yes` / `missing` / `na`. NULL until answered. |
| `supersedes` | TEXT | yes | `answer_feedback.id` of the row this replaces (NULL for first). |
| `superseded_at` | BIGINT | yes | Epoch ms; set on the prior row when replaced. NULL = **current**. |
| `created_at` | BIGINT | no | Epoch ms (matches `query_log.created_at`). |

### Constraints & indexes

```sql
CHECK (citation_accuracy     IS NULL OR citation_accuracy     IN ('all','some','wrong','unsure'))
CHECK (citation_completeness IS NULL OR citation_completeness IN ('yes','missing','na'))
CHECK (citation_accuracy IS NOT NULL OR citation_completeness IS NOT NULL)  -- at least one answer

-- Only one CURRENT row per (message_id, user). NULLs distinct in PG, so scope by user via COALESCE
-- key or a partial index over current rows:
CREATE UNIQUE INDEX uq_answer_feedback_current
  ON answer_feedback (message_id, COALESCE(user_id, ''))
  WHERE superseded_at IS NULL;

CREATE INDEX idx_answer_feedback_message ON answer_feedback (message_id);
CREATE INDEX idx_answer_feedback_created ON answer_feedback (created_at);
```

No foreign key is declared (consistent with `query_log`, which also stores `conversation_id`/
`user_id` without FKs to tolerate desktop/no-auth and best-effort logging).

### Enum value → officer-facing label (labels live in the frontend, per spec Clarifications)

**Citation accuracy**

| Stored code | Label shown |
|-------------|-------------|
| `all` | All citations correct |
| `some` | Some citations correct |
| `wrong` | Citations wrong/missing |
| `unsure` | Not sure |

**Citation completeness**

| Stored code | Label shown |
|-------------|-------------|
| `yes` | Fully cited |
| `missing` | Some citations missing |
| `na` | Not applicable |

## Write semantics (repository)

`upsert(pool, message_id, conversation_id, user_id, accuracy?, completeness?)`:

1. Find current row for `(message_id, user_id)` (WHERE `superseded_at IS NULL`).
2. If none → INSERT new current row with the provided value(s), `supersedes = NULL`.
3. If exists → copy forward the other (still-set) column, stamp `superseded_at = now` on the old row,
   INSERT a new current row with `supersedes = <old id>` and the merged values.
4. Best-effort: errors are logged, never surfaced to the chat turn (feedback is non-blocking).

This yields: independent answering of the two questions (D4), latest-wins with full history (D3).

## Read semantics

- **Per-message current feedback** (to re-hydrate the UI on reload): newest current row for
  `(message_id, user_id)`.
- **Admin summary** (FR-009): over current rows, `GROUP BY citation_accuracy` and
  `GROUP BY citation_completeness`, optionally bucketed by day and by agent/engine (join via
  `conversation_id` → `query_log` when engine A/B lands, FR-010).

## Deferred entities (not modeled in this plan)

- **Implicit Signal** (US2) — sibling table `answer_signal(message_id, type, created_at)` later.
- **SME Review / Correction**, **Golden / Regression Set Membership** (US3) — separate plan.
- **Per-citation feedback** (FR-005, v1.1) — would add `citation_index` granularity.
