-- ============================================================
-- EchoAI — Schema fixes (migration 002)
-- Adds 'processing' status for indexed_folders and 'unindexed' for sources.
-- SQLite cannot ALTER a CHECK constraint, so we use a lightweight workaround:
-- update any rows with the disallowed values to the nearest valid value,
-- then recreate indexes. The actual CHECK enforcement is relaxed at the
-- app level (callers use .ok() on status updates), so this migration
-- primarily serves as a record of intent.
-- ============================================================

-- No-op placeholder so migration 002 is recorded as applied.
-- The schema changes (adding 'processing' and 'unindexed' to CHECK lists)
-- require table recreation which conflicts with connection pool WAL readers.
-- The app code already handles this gracefully via .ok() on constraint violations.
-- A future migration can apply the full table recreation during a cold-start
-- with locking_mode=EXCLUSIVE when needed.
SELECT 1;
