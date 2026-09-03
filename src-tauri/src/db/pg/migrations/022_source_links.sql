-- Index a user's document ONCE; every later attachment of the same bytes is a
-- link, not a second copy.
--
-- Today `attach` writes each upload to `uploads/<user>/__conv__<conv>/<name>`
-- and the indexer dedups on `file_path` (`indexer.rs` `get_by_path`). A
-- per-conversation directory means a different path every time, so attaching one
-- spreadsheet to twelve chats produced twelve `sources` rows and twelve full
-- chunk+embedding sets of identical content. `file_hash` could have caught it,
-- but the attach endpoint pre-creates its row with an EMPTY hash and the
-- re-index branch never backfills it, so the existing `idx_sources_hash` indexes
-- a blank column for exactly the rows that duplicate.
--
-- The fix needs one source row to be reachable from N conversations, and
-- `sources.conversation_id` is a single column. So: that column keeps its
-- current meaning -- the conversation a source ORIGINATED in, which is what
-- every `conversation_id IS NULL` corpus predicate already reads it as, and
-- those all keep working untouched -- and this new table becomes the
-- authoritative set of conversations a source is reachable FROM.
--
-- Scope is PER-USER by decision (2026-08-04). Two different users uploading the
-- same bytes still get their own row and their own chunks: cross-user linking
-- would make one user inherit another's extraction and leak the fact that the
-- file was already in the system, and that needs a spec + ADR of its own.
-- `owner_id` on the link row is what keeps that boundary local to this table.
--
-- ADDITIVE ONLY at the schema level. ⚠️ As with 020/021, that is NOT the same as
-- a reversible deploy: `init_schema` runs the sqlx migrator with
-- `ignore_missing = false`, so once this has applied, a pre-022 binary fails at
-- startup with `VersionMissing`. Rolling the code back requires deleting this
-- row from `_sqlx_migrations` first; the table itself is inert to older code.

CREATE TABLE IF NOT EXISTS source_link (
    source_id       TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    conversation_id TEXT NOT NULL,
    -- Denormalised from `sources.owner_id` on purpose: retrieval scoping matches
    -- on (conversation_id, owner_id) together, and pairing them on the link row
    -- keeps that a single index lookup with no join back to `sources`. It is
    -- also the column that stops a guessed conversation id from reaching another
    -- user's attachments -- the same reason `visible_source_ids` pairs them.
    owner_id        TEXT NOT NULL,
    linked_at       BIGINT NOT NULL,
    PRIMARY KEY (source_id, conversation_id)
);

-- The retrieval lookup: "what is reachable in this conversation, for this user".
CREATE INDEX IF NOT EXISTS idx_source_link_conv
    ON source_link (conversation_id, owner_id);

-- The delete lookup: "does anything still link to this source" -- the check that
-- stops deleting one chat from destroying content another chat still uses.
CREATE INDEX IF NOT EXISTS idx_source_link_source
    ON source_link (source_id);

-- Backfill: every existing attachment becomes a link to the conversation it
-- already belongs to, so scoping through the link table on the very first
-- request after deploy sees exactly what the column-based scoping saw. Rows with
-- a NULL owner cannot be scoped to a user and are left alone (desktop-era rows;
-- `visible_source_ids` never matched them on the attachment branch either).
INSERT INTO source_link (source_id, conversation_id, owner_id, linked_at)
SELECT id, conversation_id, owner_id, created_at
FROM sources
WHERE conversation_id IS NOT NULL AND owner_id IS NOT NULL
ON CONFLICT (source_id, conversation_id) DO NOTHING;

-- Content dedup lookup, scoped to one owner. Partial because the pre-022
-- attachment rows carry an empty hash and must never collide with each other:
-- ('', ownerA) is not a content match, it is an absence of one.
CREATE INDEX IF NOT EXISTS idx_sources_owner_hash
    ON sources (owner_id, file_hash)
    WHERE file_hash <> '' AND owner_id IS NOT NULL;
