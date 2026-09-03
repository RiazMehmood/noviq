-- Landing page — "Request a quote": an organisation asking to be contacted.
--
-- ⚠️ This is the FIRST table in the system written by an UNAUTHENTICATED caller.
-- Every other write path sits behind `require_auth`; this one is reachable by
-- anyone who can load the public page. Three consequences are designed in here
-- rather than left to the handler:
--
--  1. NOTHING in this table is ever interpolated into a prompt, a query, or a
--     shell command. It is contact text typed by a stranger. It is displayed in
--     the admin console and mailed to the admin, and that is all.
--  2. Every free-text column is length-bounded at the schema level, so a bad
--     handler (or a future second caller) cannot fill the disk. The API bounds
--     them too; this is the backstop, not the primary check.
--  3. `source_ip` exists ONLY for rate limiting and abuse triage. It is personal
--     data: it is never shown in the admin list, only counted. Prune with the
--     rows themselves.
--
-- Additive. As with 023, once applied a pre-028 binary fails at startup with
-- `VersionMissing` (the migrator runs with `ignore_missing = false`), so a code
-- rollback means deleting this row from `_sqlx_migrations` first.

CREATE TABLE IF NOT EXISTS quote_request (
    id              TEXT PRIMARY KEY,
    -- Contact block. `organisation` and `email` are the only required fields:
    -- asking a ministry for a phone number before it will talk to you loses the
    -- lead, and every other field is qualification detail we would LIKE.
    organisation    TEXT NOT NULL CHECK (length(organisation) BETWEEN 1 AND 200),
    contact_name    TEXT          CHECK (contact_name IS NULL OR length(contact_name) <= 120),
    email           TEXT NOT NULL CHECK (length(email) BETWEEN 3 AND 254),
    phone           TEXT          CHECK (phone IS NULL OR length(phone) <= 40),
    -- Which of the landing page's five tracks they came from: government,
    -- academia, industry, developers, policy. Free text rather than an enum so
    -- adding a sixth track on the page needs no migration; the API validates.
    sector          TEXT          CHECK (sector IS NULL OR length(sector) <= 40),
    -- Rough size / seat count, as the organisation describes it.
    org_size        TEXT          CHECK (org_size IS NULL OR length(org_size) <= 40),
    message         TEXT          CHECK (message IS NULL OR length(message) <= 4000),

    -- Triage state. `new` -> `read` -> `replied` / `archived`. Deliberately not
    -- an enum type: statuses are workflow, and workflow changes more often than
    -- schemas.
    status          TEXT NOT NULL DEFAULT 'new'
                      CHECK (status IN ('new', 'read', 'replied', 'archived')),
    -- Free-text note the admin adds while working the lead.
    admin_note      TEXT          CHECK (admin_note IS NULL OR length(admin_note) <= 4000),

    -- BIGINT epoch MILLISECONDS, matching `sources` and migration 023.
    -- ⚠️ Writing seconds here silently records the year 58597 rather than
    -- failing -- that has happened in this schema before.
    created_at      BIGINT NOT NULL,
    updated_at      BIGINT NOT NULL,

    -- Abuse triage only. Never rendered in the admin list.
    source_ip       TEXT          CHECK (source_ip IS NULL OR length(source_ip) <= 64),
    -- Whether the notification email actually went out. NULL = not attempted
    -- (no SMTP configured). The row is authoritative either way: the admin
    -- inbox never depends on mail having worked.
    emailed_at      BIGINT
);

-- The inbox view: newest first, unread first.
CREATE INDEX IF NOT EXISTS idx_quote_request_created ON quote_request (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_quote_request_status  ON quote_request (status);

-- Rate limiting reads "how many from this address recently", so it must not be
-- a sequential scan once the table is large.
CREATE INDEX IF NOT EXISTS idx_quote_request_ip_time ON quote_request (source_ip, created_at DESC);
