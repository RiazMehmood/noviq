# Phase 1 Data Model — Server / Thin-Client Deployment

Additive migration (does **not** bump `TARGET_SCHEMA_VERSION` in a breaking way; nullable adds only,
so an older binary remains safe on the newer schema → rollback-safe, FR-017).

## New: `users`
| field | type | notes |
|-------|------|-------|
| id | text PK | uuid string (matches existing TEXT-id convention) |
| username | text UNIQUE NOT NULL | login |
| password_hash | text NOT NULL | argon2id; never returned to client |
| created_at | bigint NOT NULL | epoch seconds (matches existing tables; IDs are TEXT) |
| role | text NOT NULL default 'user' | `admin` \| `user` — **enforced** for admin endpoints (US7, FR-020) |
| disabled_at | bigint NULL | non-NULL = disabled (US7); login rejected + sessions invalidated |
| department_id | text NULL | **reserved** for RBAC follow-on (FR-010) |

**Admin bootstrap (FR-019):** on first boot, if no `role='admin'` row exists, seed one from
`NOVIQ_ADMIN_USER`/`NOVIQ_ADMIN_PASS` (argon2id). Idempotent.

## New: `session`
| field | type | notes |
|-------|------|-------|
| id | text PK | uuid string |
| user_id | text NOT NULL → users.id ON DELETE CASCADE | |
| token_hash | text NOT NULL | hash of the bearer token; raw token never stored |
| created_at | bigint NOT NULL | epoch seconds |
| expires_at | bigint NOT NULL | expiry/logout revocation |
| last_seen_at | bigint NULL | rolling activity |

Validation: token random ≥256-bit; lookup by `token_hash`; reject if expired/absent → `401`.
Login: rate-limit per username/IP; lockout after N failures (brute-force protection, FR-004).

## Altered: `sources` (existing) — reserved RBAC columns (nullable, unenforced)
| field | type | notes |
|-------|------|-------|
| department_id | text NULL | reserved (FR-010) |
| owner_user_id | uuid NULL | reserved |
| visibility | text NULL | reserved: `private` \| `department`; NULL = current shared behavior |

## Altered: `audit_log` (existing) — multi-user traceability (FR-011)
| field | type | notes |
|-------|------|-------|
| user_id | uuid NULL → users.id | who performed the action |
| action | text | e.g. `query`, `index`, `export`, `login`, `deploy` |
| target | text NULL | doc id / source / endpoint |

## Identity threading (FR-010, no enforcement yet)
Every authenticated request resolves `user_id` from the session and passes it into the
retrieval/drafting/indexing service calls as a context value. This feature **does not filter** by
it; it is the seam the RBAC follow-on uses to add ACL filtering at the retrieval layer without
re-plumbing.

## Not in the DB
- `models/*.gguf` — server filesystem, out-of-band (rsync), never in git.
- Secrets (`$DEPLOY_SECRET` if webhook ever enabled, DB url, GPU layer count) — `.env` via systemd
  `EnvironmentFile`, git-ignored.
- `ir_cache/`, `page_cache/` — server filesystem, co-located with DB (FR-007).
</content>
