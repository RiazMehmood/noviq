# Data Model — 021 User Documents

Phase 1 output. One additive migration; no table drops, no re-index.

## Migration `009_sources_ownership.sql` (PG) + SQLite mirror

### `sources` — new columns (retrieval scope truth)

| Column | Type | Default | Meaning |
|--------|------|---------|---------|
| `owner_id` | TEXT NULL, FK → users.id (soft) | NULL | Owning user; NULL = system/admin corpus |
| `visibility` | TEXT NOT NULL CHECK IN ('private','shared') | 'shared' | Whether all users may retrieve from it |
| `conversation_id` | TEXT NULL | NULL | Non-NULL ⇒ conversation attachment (US4) |

Indexes: `idx_sources_owner (owner_id)`, `idx_sources_conversation (conversation_id)`,
partial `idx_sources_shared ON sources(id) WHERE visibility='shared' AND conversation_id IS NULL`.

### `indexed_folders` — new column (UI/default only)

| Column | Type | Default | Meaning |
|--------|------|---------|---------|
| `visibility` | TEXT NOT NULL CHECK IN ('private','shared') | 'shared' | Folder-level display + default for new files; flips cascade to child sources |

### Backfill (in the migration, one transaction)

```sql
-- owner: inherit from owning folder through the stub-notebook link
UPDATE sources s SET owner_id = f.owner_id
FROM notebook_sources ns, indexed_folders f
WHERE ns.source_id = s.id AND ns.notebook_id = '__folder__' || f.id
  AND f.owner_id IS NOT NULL;

-- visibility: user-owned folders were "private" in intent; global corpus stays shared
UPDATE sources s SET visibility = 'private'
FROM notebook_sources ns, indexed_folders f
WHERE ns.source_id = s.id AND ns.notebook_id = '__folder__' || f.id
  AND f.owner_id IS NOT NULL;

UPDATE indexed_folders SET visibility = CASE WHEN owner_id IS NULL THEN 'shared' ELSE 'private' END;
```

Chunks, embeddings, FTS, graph: untouched (FR-015 — no re-index).

## Scope rule (single function, the privacy invariant)

`visible_source_ids(pool, scope)` returns ids where:

```
   (visibility = 'shared' AND conversation_id IS NULL)                  -- global corpus
OR (owner_id = :user_id  AND conversation_id IS NULL)                   -- my documents
OR (conversation_id = :conversation_id)                                 -- this conversation's attachments
```

- `RequestScope { user_id: Option<String>, conversation_id: Option<String> }`
- `RequestScope::unrestricted()` (desktop/Tauri, single operator) → all indexed sources,
  today's behaviour.
- Admin gets NO special case here (user decision 2026-07-05): admin answers = shared + own.

## State transitions

```
upload (US2):        (owner=user,  visibility=private, conversation=NULL)
admin upload:        (owner=NULL,  visibility=shared,  conversation=NULL)   ← unchanged behaviour
share (US3):         visibility private → shared        (owner keeps ownership)
unshare (US3):       visibility shared → private        (blocked for owner_id IS NULL? No — admin may private a corpus doc too)
attach (US4):        (owner=user,  visibility=private, conversation=<id>)
promote (US4):       conversation <id> → NULL; source re-linked to user's uploads folder notebook; file moved
delete:              existing FK-safe cascade (embeddings → FTS → graph → chunks → notebook_sources → sources) + file removal
delete_conversation: apply `delete` to every source with conversation_id = <id>
```

Invariants:
- I1: `conversation_id IS NOT NULL ⇒ owner_id IS NOT NULL AND visibility = 'private'`.
- I2: A source appears in at most one stub notebook (`__folder__…` or `__conv__…`); promotion swaps the link atomically.
- I3: Folder visibility flip updates the folder row and ALL its child sources in one transaction.
- I4: Deduplication (`file_hash`) never merges rows across owners (spec edge case) — dedup lookups add `owner_id IS NOT DISTINCT FROM :owner`.

## Allowance (no new table)

- Limits: `NOVIQ_USER_QUOTA_MB` (default 500), `NOVIQ_USER_QUOTA_FILES` (default 200).
- Usage: `SUM(file_size_bytes), COUNT(*) FROM sources WHERE owner_id = :user`
  (attachments included while alive — FR-019).
- Enforced pre-save in upload + attach handlers; refusal body carries usage + limit (FR-009).

## Audit events (existing `user_events` table, new `event_type` values)

`upload` (exists) · `delete_document` · `share` · `unshare` · `attach` · `promote` ·
`delete_conversation` · `admin_delete_document` — each with acting user id + target id (FR-011).
