-- Remember which category the RUNNING batch is filing into.
--
-- The bug this fixes, seen on the first real corpus load (2026-08-14): a folder
-- of 157 documents was uploaded into "BOARD OF REVENUE" and every one of them
-- landed in Uncategorised.
--
-- The category is applied per file as that file FINISHES indexing. Automatic GPU
-- indexing restarts the server about a minute into a batch (the ONNX runtime
-- switch is read at exec), which kills every in-flight task;
-- `resume_interrupted_indexing` re-queues them at startup but had no idea what
-- category the batch was for, so it passed None. Since the restart lands early,
-- that was effectively the whole batch.
--
-- Holding it on the folder row is the smallest thing that survives the restart,
-- and it is scoped to the batch exactly like `indexing_total` / `indexing_done`:
-- set by `begin_batch`, read only while a batch is in flight.
--
-- ON DELETE SET NULL for the same reason as `sources.category_id` in 021:
-- deleting a category must never break anything, it just stops filing.
--
-- ADDITIVE ONLY. ⚠️ Additive is still not a reversible deploy — `init_schema`
-- runs the migrator with `ignore_missing = false`, so a pre-027 binary fails
-- startup with `VersionMissing` until this row is removed from
-- `_sqlx_migrations`. The column itself is inert to older code.

ALTER TABLE indexed_folders
    ADD COLUMN IF NOT EXISTS indexing_category_id TEXT
    REFERENCES corpus_category(id) ON DELETE SET NULL;
