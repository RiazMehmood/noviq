# Phase 1 Contracts — HTTP API surface

All endpoints under `/api`. JSON in/out. Every endpoint except `/api/health` and `/api/login`
requires `Authorization: Bearer <token>` (or the session cookie); missing/invalid → `401`. Clients
never connect to PostgreSQL directly (FR-006/SC-004).

## Auth & lifecycle
| Method | Path | Body → Response | Notes |
|--------|------|-----------------|-------|
| POST | `/api/login` | `{username,password}` → `{token,expires_at}` | argon2id verify; rate-limited (FR-004) |
| POST | `/api/logout` | — → `204` | revoke session |
| GET | `/api/me` | — → `{id,username}` | identity echo (FR-010) |
| GET | `/api/health` | — → `{status:"ok",gpu:bool,layers:int,version}` | unauthenticated; deploy health probe (FR-017) |

## Feature parity (FR-012) — one endpoint per existing Tauri command
Map each `src-tauri/src/commands/*.rs` command to `POST /api/<command>` with the same params,
routed through the shared service functions (no logic duplicated). Pilot scope:
| Area | Commands (existing) → endpoints |
|------|---------------------------------|
| chat/retrieval | `query`, `search` → `/api/query`, `/api/search` |
| drafting | `one_ask`, `drafting` → `/api/one_ask`, `/api/drafting/*` |
| indexing | `importer`, `sources`, `folders` → `/api/index`, `/api/sources`, `/api/folders` (progress via SSE/WebSocket `/api/index/events`) |
| viewing | `analysis`, `entities`, `audit` → `/api/...` (read) |

**Indexing concurrency (FR-008):** index requests enqueue on a bounded worker; per-job progress
streams to the requesting client over `/api/index/events` (replaces Tauri `importer://progress`).

**Errors:** structured `{error:{code,message}}`, never a silent IPC failure (US1 acceptance).

## Admin — user management (US7, admin-only → `403` for non-admins)
| Method | Path | Body → Response | Notes |
|--------|------|-----------------|-------|
| GET | `/api/admin/users` | — → `[{id,username,role,disabled_at,created_at}]` | list all |
| POST | `/api/admin/users` | `{username,password}` → `{id,username}` | create tester (role `user`) |
| POST | `/api/admin/users/:id/disable` | — → `204` | set `disabled_at`; revoke sessions |
| POST | `/api/admin/users/:id/enable` | — → `204` | clear `disabled_at` |

Admin is env-seeded on first boot (FR-019). A role guard (FR-020) rejects non-admin sessions with
`403`. Disabled users (FR-021): login rejected + existing sessions invalidated.

## Deploy (ops, not a client API)
- **Primary:** no HTTP endpoint — the poll-timer drives deploys (see `deployment-and-updates.md`).
- **Optional future:** `POST /__deploy` (HMAC-SHA256 `X-Hub-Signature-256`; verify
  `ref==refs/heads/release`; `202`; triggers `noviq-deploy.service`). Disabled unless a public
  endpoint exists.

## Frontend transport seam (FR-001/FR-003)
`src/lib/transport.ts`: `call(cmd, args)` → Tauri `invoke(cmd,args)` if `window.__TAURI__` else
`fetch('/api/'+cmd, {method:'POST', body:JSON.stringify(args), headers:{Authorization}})`. UI pages
import `call`, never `invoke`.
</content>
