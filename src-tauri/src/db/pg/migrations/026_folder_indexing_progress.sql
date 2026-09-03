-- Observable indexing progress for a batch of uploads.
--
-- The problem this fixes: an admin selects a folder of 61 PDFs in the browser,
-- the request blocks while every file is buffered and `pdftotext`-probed, then
-- returns — and nothing moves again. Indexing itself runs in spawned tasks whose
-- only progress channels are (a) a Tauri event that is a documented no-op when
-- `app` is None, i.e. always on the server, and (b) an SSE stream the Cloudflare
-- quick tunnel buffers indefinitely. The upload response also carries no source
-- ids (`index_sources` returns `id: String::new()`), so the tunnel-safe
-- `index_progress {sourceIds}` poll has nothing to ask about either.
--
-- So progress is recorded HERE, on the folder row, and read back by polling.
-- That is deliberate rather than a stream: the admin who reported this refreshed
-- the tab and only then saw the count, and committed state is the only thing
-- that survives a refresh, a tab switch, or the tunnel.
--
-- `file_count` is left alone — it is derived per-viewer in `folders::list*` as a
-- COUNT over indexed sources and means "documents in this folder". These two
-- mean "the batch running right now", which is a different question:
--   indexing_total = files in the current batch (0 when idle)
--   indexing_done  = files finished so far, bumped atomically per file
-- Idle is (total = 0), NOT (done = total) — a finished batch leaves done = total
-- until the next one starts, and the UI needs to tell those apart.
--
-- ADDITIVE ONLY. ⚠️ As with 021/025, additive is not the same as a reversible
-- deploy: `init_schema` runs the migrator with `ignore_missing = false`, so once
-- applied, a pre-026 binary fails at startup with `VersionMissing`. Rolling back
-- the code means deleting this row from `_sqlx_migrations` first; the columns
-- are inert to older code and can stay.

ALTER TABLE indexed_folders
    ADD COLUMN IF NOT EXISTS indexing_total BIGINT NOT NULL DEFAULT 0;

ALTER TABLE indexed_folders
    ADD COLUMN IF NOT EXISTS indexing_done BIGINT NOT NULL DEFAULT 0;
