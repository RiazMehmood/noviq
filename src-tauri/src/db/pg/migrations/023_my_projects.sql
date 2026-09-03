-- Feature 027 — "My Projects": user-owned document workspaces.
--
-- A project is a named container holding its own documents and many chats, where
-- retrieval is scoped to that project's documents and NOTHING else — the first
-- requirement in this system's history that must SUBTRACT rather than add.
-- See ADR-0029 for the scoping seam and ADR-0028 for the sharing model.
--
-- ─── Why new tables rather than reusing `notebooks` ──────────────────────────
-- `notebook_sources` has exactly the membership shape a project needs
-- (notebook_id, source_id, added_at). That is the trap. `notebooks` is LIVE and
-- already carries two distinct meanings: `api/uploads.rs` mints a stub notebook
-- per CONVERSATION, `api/documents.rs` mints one per FOLDER (`__folder__<id>`),
-- four delete paths unlink them, and `query_sessions.notebook_id` FKs to it.
-- A project would be a third meaning on the same rows, needing a discriminator
-- that four existing delete paths do not know to check. Sibling tables instead.
--
-- ─── Why cross-user linking is permitted here ───────────────────────────────
-- Migration 022 restricted `source_link` to a single user and said cross-user
-- linking "needs a spec + ADR of its own". Feature 027 is that spec; ADR-0028 is
-- that ADR. The hazard 022 named is *implicit* dedup — B uploads a file, is
-- silently linked to A's row, and thereby learns the file was already indexed.
-- Copy-on-share is EXPLICIT and owner-initiated, so nothing is disclosed that
-- the sharer did not choose to disclose. `source_link` stays per-user exactly as
-- 022 built it; only `project_source` below may cross users, and only through a
-- deliberate share.
--
-- ⚠️ ADDITIVE at the schema level, which is NOT the same as a reversible deploy.
-- `init_schema` runs the sqlx migrator with `ignore_missing = false`, so once
-- this has applied a pre-023 binary fails at startup with `VersionMissing`.
-- Rolling the code back requires deleting this row from `_sqlx_migrations`
-- first; the tables themselves are inert to older code.

-- ─── project ────────────────────────────────────────────────────────────────
-- Exactly one owner, always. Sharing never grants access to a row here; it
-- INSERTS a second row owned by the recipient (ADR-0028).
CREATE TABLE IF NOT EXISTS project (
    id          TEXT PRIMARY KEY,
    owner_id    TEXT NOT NULL,
    name        TEXT NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL,
    -- Provenance ONLY: the project this one was copied from. Deliberately NOT a
    -- foreign key. An FK with ON DELETE CASCADE would destroy the recipient's
    -- project when the sharer deleted theirs -- precisely the failure FR-024
    -- forbids -- and even ON DELETE SET NULL would imply a live relationship
    -- that does not exist. Never read by any scoping or deletion query, and may
    -- point at a row that no longer exists.
    copied_from TEXT
);

-- The FR-004 listing: "my projects, most recently active first".
CREATE INDEX IF NOT EXISTS idx_project_owner ON project (owner_id, updated_at DESC);

-- ─── project_source ─────────────────────────────────────────────────────────
-- Membership. Many-to-many and CROSS-USER: a row may name a project owned by B
-- and a source owned by A.
--
-- ⚠️ There is deliberately NO `owner_id` column here, unlike `source_link`. On
-- `source_link`, `owner_id` is the guard that stops a guessed conversation id
-- reaching another user's attachments. Here the guard sits one level up --
-- `project.owner_id` is checked before any scope is built (`owned_project`) --
-- and membership alone then decides what is visible. An `owner_id` here could
-- only record the SOURCE's owner, which for a copied project is a different
-- user, so any predicate matching it against the caller would break exactly the
-- sharing this table exists to provide. Its absence is the design, not an
-- oversight (ADR-0028).
CREATE TABLE IF NOT EXISTS project_source (
    project_id TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    source_id  TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    added_at   BIGINT NOT NULL,
    -- Re-adding a document already in the project is idempotent, not an error.
    PRIMARY KEY (project_id, source_id)
);

-- The retrieval lookup: "what is in this project" -- the hot path for every
-- project question.
CREATE INDEX IF NOT EXISTS idx_project_source_project ON project_source (project_id);

-- The delete-safety lookup: "does any project still hold this source". After
-- this migration that question can be answered YES by a project belonging to a
-- DIFFERENT user, which is why every delete path must consult it.
CREATE INDEX IF NOT EXISTS idx_project_source_source ON project_source (source_id);

-- ─── project_chat ───────────────────────────────────────────────────────────
-- The mapping this system had nowhere to put. There is no conversation table:
-- `query_sessions`/`query_messages` exist in the 001 baseline but have ZERO
-- references under `src-tauri/src/api/` (vestigial desktop path), and the live
-- web identity is `query_log.conversation_id` -- a client-minted string with no
-- owning row.
--
-- That absence is turned into the security property: this row is written
-- SERVER-SIDE when a chat is started in a project, so on a chat turn the client
-- sends only `conversation_id` and the server DERIVES the project. A forged or
-- guessed conversation id finds no row, or another user's row that the
-- `owner_id` check rejects. The chat path therefore cannot be widened at all
-- (ADR-0029).
CREATE TABLE IF NOT EXISTS project_chat (
    conversation_id TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    -- Denormalised from `project.owner_id` so the per-turn ownership check is a
    -- single index lookup with no join back to `project`.
    owner_id        TEXT NOT NULL,
    created_at      BIGINT NOT NULL
);

-- The FR-033 chat listing for one project.
CREATE INDEX IF NOT EXISTS idx_project_chat_project ON project_chat (project_id, created_at DESC);

-- ─── chat_share ─────────────────────────────────────────────────────────────
-- A read-only link to one chat. Openable ONLY by a user signed in to Noviq with
-- their own credentials -- the token is not an authentication mechanism, it is a
-- selector; `AuthUser` is still required (FR-027).
CREATE TABLE IF NOT EXISTS chat_share (
    -- The whole credential: >= 128 bits of entropy, minted server-side.
    token           TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    owner_id        TEXT NOT NULL,
    created_at      BIGINT NOT NULL,
    -- Soft revocation, matching how chat deletion already works. NULL = live.
    -- Copies already taken from a link survive its revocation, because a copy is
    -- an independent project with no reference back (FR-031).
    revoked_at      BIGINT
);

-- At most ONE live link per chat, so re-sharing returns the existing token
-- rather than accumulating credentials that would all have to be revoked
-- separately. Partial, so revoked rows are retained as history without
-- colliding.
CREATE UNIQUE INDEX IF NOT EXISTS idx_chat_share_live
    ON chat_share (conversation_id)
    WHERE revoked_at IS NULL;

-- The owner's "is this chat shared" lookup, and the revoke path.
CREATE INDEX IF NOT EXISTS idx_chat_share_conv ON chat_share (conversation_id);

-- ─── availability gate ──────────────────────────────────────────────────────
-- Seeded OFF, so applying this migration exposes NOTHING until an administrator
-- turns it on. Deployment-wide, read server-side on every project request; the
-- UI never being rendered is not the enforcement (FR-038). `DO NOTHING` so a
-- re-run cannot switch a live deployment back off.
INSERT INTO db_config (key, value) VALUES ('my_projects_enabled', '0')
ON CONFLICT (key) DO NOTHING;
