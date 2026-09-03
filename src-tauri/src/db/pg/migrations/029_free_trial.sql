-- Free trial: a member of the public signs in with Google and gets a strictly
-- limited look at Noviq — 3 questions a day, for 3 days, over a curated set of
-- published documents, with Chat and Search only.
--
-- ─── Why trial users are ordinary `users` rows ──────────────────────────────
-- They reuse `users` (role = 'trial') and therefore reuse sessions, `AuthUser`,
-- the audit trail and every existing ownership check. A parallel identity table
-- would mean a second auth path, and a second auth path is a second place to
-- get authorisation wrong. `users.role` is TEXT with no CHECK, so 'trial' needs
-- no schema change; what needs saying is that **every role test in the codebase
-- that means "a real user" must now exclude 'trial' explicitly** — `role !=
-- 'admin'` is no longer the same question as "is this a paying customer".
--
-- ─── The three limits, and where each is enforced ───────────────────────────
--   1. 3 questions per UTC day   -> `trial_usage`, checked in the query path
--   2. 3 days from first sign-in -> `trial_account.started_at`, same check
--   3. Chat + Search only        -> role test on the other API surfaces
-- None of them is a UI concern. The landing page describes them; the server
-- refuses them. This codebase has already learned once that hiding a control is
-- presentation and never a gate (deep search, 2026-07).
--
-- Additive, and the same one-way property as 023/028: once applied, a pre-029
-- binary fails at startup with `VersionMissing`.

-- ── Who the trial user is ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS trial_account (
    user_id     TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    -- Google's stable subject id. This, NOT the email, is the identity: Google
    -- documents `sub` as the only claim that never changes, while an email can
    -- be reassigned within a Workspace domain. UNIQUE so one Google account can
    -- never hold two trials.
    google_sub  TEXT NOT NULL UNIQUE,
    -- Kept for contact and for the admin to see who tried the product. Not an
    -- identity key: deliberately NOT unique.
    email       TEXT NOT NULL,
    -- Display name as Google reported it, for the admin list. May be absent.
    display_name TEXT,
    -- Epoch MILLISECONDS. ⚠️ Seconds here would silently place the trial's start
    -- in the year 58597 and grant an unlimited trial — the same millisecond trap
    -- `sources` already carries.
    started_at  BIGINT NOT NULL,
    -- Set when an admin cuts a trial short. Checked alongside `users.disabled_at`.
    revoked_at  BIGINT
);

CREATE INDEX IF NOT EXISTS idx_trial_account_email ON trial_account (email);

-- ── What they have used, per UTC day ────────────────────────────────────────
-- One row per (user, day). `day_utc` is a TEXT 'YYYY-MM-DD' rather than a date
-- range so the quota question is a single primary-key lookup, and so the day
-- boundary is unambiguous and identical for everyone regardless of where they
-- are. A trial user in Karachi and one in London share the same reset instant.
CREATE TABLE IF NOT EXISTS trial_usage (
    user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day_utc  TEXT NOT NULL CHECK (length(day_utc) = 10),
    used     INTEGER NOT NULL DEFAULT 0 CHECK (used >= 0),
    PRIMARY KEY (user_id, day_utc)
);

-- ── What a trial user is allowed to retrieve ────────────────────────────────
-- The curated demo corpus (architect's decision, 2026-08-25): a small set of
-- already-published instruments. A trial user is an anonymous member of the
-- public, and the live corpus holds 3,931 Government of Sindh documents, some
-- of them internal.
--
-- ⚠️⚠️ THIS TABLE FAILS CLOSED, AND MUST CONTINUE TO. An EMPTY `demo_source`
-- means a trial user can retrieve NOTHING — it must never be read as "no
-- restriction". The scoping helper returns an explicit empty allow-list rather
-- than `None`, because `None` means "everything" everywhere else in the
-- retrieval path. Getting this backwards would publish the entire corpus to
-- anyone with a Google account.
CREATE TABLE IF NOT EXISTS demo_source (
    source_id TEXT PRIMARY KEY REFERENCES sources(id) ON DELETE CASCADE,
    added_at  BIGINT NOT NULL
);
