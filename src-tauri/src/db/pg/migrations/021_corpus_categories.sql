-- Admin-controlled corpus organisation: categories + per-document disable.
--
-- The corpus is one flat, unlabelled list today, so an officer browsing it sees
-- nineteen instruments with nothing to group them and no way to tell which are
-- actually in service. This adds both, as METADATA ONLY.
--
-- Nothing here moves a file on disk, re-chunks, re-embeds or re-indexes. A
-- category is a column on `sources`; a disable is a timestamp on `sources`,
-- enforced at retrieval time in `visible_source_ids` (which is recomputed on
-- every turn — the same property that makes F021 visibility take effect without
-- a re-index).
--
-- ADDITIVE ONLY at the schema level. ⚠️ As with 020, that is NOT the same as a
-- reversible deploy: `init_schema` runs the sqlx migrator with
-- `ignore_missing = false`, so once this has applied, a pre-021 binary fails at
-- startup with `VersionMissing`. Rolling the code back requires deleting this
-- row from `_sqlx_migrations` first; the columns themselves are inert to older
-- code and can stay.

CREATE TABLE IF NOT EXISTS corpus_category (
    id          TEXT PRIMARY KEY,
    -- UNIQUE so two categories cannot present the same label to an officer; the
    -- admin UI renames rather than creating a second "Finance".
    name        TEXT NOT NULL UNIQUE,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  BIGINT NOT NULL
);

-- One category per document (the architect's decision, 2026-08-01): a document
-- belongs to exactly one, like a folder, so "which category is this in" always
-- has a single answer. NULL = Uncategorised, which is also where every existing
-- document starts.
--
-- ON DELETE SET NULL is the safety property that matters: deleting a category
-- must never delete documents. They fall back to Uncategorised, and the admin
-- can re-file them.
ALTER TABLE sources ADD COLUMN IF NOT EXISTS category_id TEXT
    REFERENCES corpus_category(id) ON DELETE SET NULL;

-- NULL = in service. A timestamp rather than a boolean because it records WHEN
-- the document was withdrawn, which the admin list shows and a boolean could
-- not. Disabled documents remain listed for officers (greyed, marked
-- unavailable) but are never retrievable or citable — see `visible_source_ids`.
ALTER TABLE sources ADD COLUMN IF NOT EXISTS disabled_at BIGINT;

-- The corpus browser groups by category and the retrieval scope filters on
-- `disabled_at`; both run per request over the whole sources table.
CREATE INDEX IF NOT EXISTS idx_sources_category ON sources(category_id);
CREATE INDEX IF NOT EXISTS idx_sources_disabled ON sources(disabled_at);
