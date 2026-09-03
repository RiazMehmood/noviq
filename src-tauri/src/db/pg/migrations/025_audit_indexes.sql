-- 2026-08-13 — indexes for the admin audit browser.
--
-- ADDITIVE, index-only: no new tables, no new columns, no data rewritten. An
-- older binary is unaffected, so a failed deploy rolls back safely.
--
-- Why now: `audit_log` gained read-access rows (document_open / document_read /
-- document_denied / global_search), which arrive far more often than the
-- mutation events it previously held. The browser filters by actor, by action
-- and by document, always ordered by time — and only `created_at` and
-- `event_type` were indexed.
--
-- `idx_audit_created` already covers plain time-ordered paging, so it is not
-- repeated here.

-- "What did this officer do?" — the per-user timeline, newest first.
CREATE INDEX IF NOT EXISTS idx_audit_user_created ON audit_log(user_id, created_at DESC);

-- "Show me every refused read" — action facet, newest first. `action` is the
-- F014 column; `event_type` mirrors it on rows written by `insert_user_event`,
-- but older rows have only `event_type`, which `idx_audit_event` covers.
CREATE INDEX IF NOT EXISTS idx_audit_action_created ON audit_log(action, created_at DESC);

-- "Who opened this document?" — the question that motivated read logging.
CREATE INDEX IF NOT EXISTS idx_audit_target_created ON audit_log(target, created_at DESC);
