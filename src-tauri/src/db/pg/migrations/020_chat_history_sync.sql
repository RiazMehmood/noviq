-- Account-bound chat history — the transcript follows the login, not the device.
--
-- Chat history was written ONLY to the browser's localStorage (`noviq:conversations`
-- + `noviq:turns:<id>`), so signing in on a second device showed an empty rail and
-- signing out on the first lost everything. The server, however, has recorded every
-- turn since F014 in `query_log` (user_id, conversation_id, question, answer,
-- citations_json, created_at) — the history existed, it was just never readable by
-- the officer who wrote it. This migration adds the two columns that turn that audit
-- log into a replayable transcript.
--
-- ADDITIVE ONLY at the schema level: both columns are nullable with no default
-- backfill, so the previous binary's queries all still work against this table.
--
-- ⚠️ That is NOT the same as "the deploy can roll back". `init_schema` runs the
-- sqlx migrator with `ignore_missing = false` (the default), and the migrator
-- errors `VersionMissing` when the database has an applied migration the binary
-- does not carry. So once this has applied, booting a PRE-020 binary against the
-- same database fails at startup. Rolling the code back requires also deleting
-- this row from `_sqlx_migrations` (the columns themselves can stay — they are
-- inert to older code).
--
-- Ordering, on the other hand, is safe: sqlx 0.8 validates only that every
-- applied migration exists locally and has no monotonicity check, so this landing
-- on prod BEFORE the still-unshipped 018 and 019 is fine — they apply out of
-- order whenever they ship. All three touch disjoint tables.

-- Full serialized `TurnResponse` for the assistant turn. `citations_json` alone
-- cannot rebuild a reopened chat: web results, capability, scoped_to_attachment,
-- trace_id and message_id all live on the response and drive real UI (the web
-- section, the "Search all documents" widen button, draft export). NULL on every
-- pre-existing row — those replay from question/answer/citations_json alone, which
-- is exactly what the local transcript already degraded to after `trimForStorage`.
ALTER TABLE query_log ADD COLUMN IF NOT EXISTS response_json TEXT;

-- Soft delete. Deleting a chat must remove it from the officer's history on EVERY
-- device, but `query_log` is also the only record of what was asked — the admin
-- usage panel (`usage_by_user`), the activity list and the per-user export all read
-- it. A hard DELETE would let any user silently punch holes in that trail. Setting
-- `hidden_at` hides the conversation from the user-facing history reads while the
-- admin reads deliberately ignore the column.
ALTER TABLE query_log ADD COLUMN IF NOT EXISTS hidden_at BIGINT;

-- The history rail reads "this user's turns for this conversation, oldest first"
-- and "this user's most recent conversations". Both are covered by one composite
-- index; without it every rail render sequentially scans a table that grows by one
-- row per chat turn forever.
CREATE INDEX IF NOT EXISTS idx_query_log_user_conv
    ON query_log(user_id, conversation_id, created_at);
