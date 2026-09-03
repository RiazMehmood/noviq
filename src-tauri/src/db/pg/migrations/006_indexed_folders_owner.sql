-- F014: per-user ownership of indexed folders.
-- NULL owner = global/admin/desktop folder (created before ownership, or by the
-- desktop app / admin server-path indexer). Regular web users only see and
-- manage folders whose owner_id matches their user id; admins see all.
ALTER TABLE indexed_folders ADD COLUMN IF NOT EXISTS owner_id TEXT;
