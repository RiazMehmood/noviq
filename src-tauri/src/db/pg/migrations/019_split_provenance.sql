-- Provenance for an instrument cut out of a compendium.
--
-- `Compendium Rules under SLGA 2013` is twelve rule-sets in one PDF. Indexed as
-- one source, every one of them numbers its provisions from 1, so "Rule 4" is
-- ambiguous and citations name a book rather than a law. Splitting it into one
-- source per instrument fixes that, but loses two things unless they are
-- recorded here.
--
-- `split_page_offset` — after the cut, page 5 of the Election Rules is page 5 of
-- a new file and page 9 of the printed book. A citation that says "p.5" cannot
-- be checked against the physical volume. Add this offset to a chunk's page
-- number to recover the book's own page. Zero for an ordinary document.
--
-- `split_from_file` — the file the instrument was cut from, kept as plain text
-- rather than a foreign key: the original source ROW is deleted by the split
-- (otherwise its chunks keep competing in retrieval, which is the whole reason
-- for splitting), while the original FILE is retained on disk. A foreign key
-- would either block that delete or null itself out, losing the provenance at
-- exactly the moment it becomes the only record of where the text came from.
--
-- Both are additive and default to the ordinary single-document case, so every
-- existing row keeps its current meaning.

ALTER TABLE sources ADD COLUMN IF NOT EXISTS split_page_offset BIGINT NOT NULL DEFAULT 0;
ALTER TABLE sources ADD COLUMN IF NOT EXISTS split_from_file   TEXT;

CREATE INDEX IF NOT EXISTS idx_sources_split_from ON sources(split_from_file)
    WHERE split_from_file IS NOT NULL;
