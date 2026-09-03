# Feature Specification: Server / Thin-Client Deployment (Internet, GPU-backed)

**Branch (suggested):** `014-server-thin-client`
**Status:** SCOPED — spec only. Supersedes the LAN remote-desktop stopgap (Option B); this is
"Path 2" from the 2026-06-20 deployment discussion.

## Clarifications

### Session 2026-06-25 (update-path amendment)

- **Q: Deploy trigger — webhook or poll?** The IBA VM is reachable only through the Easy Connect
  **VPN at a private address (~10.150.x.100)**; confirmed this session by connecting via VPN→SSH.
  A private VPN address has **no inbound path from GitHub**, so a push webhook cannot reach it.
  **Decision: the server-side poll-timer is the PRIMARY deploy trigger** (polls `origin/release`,
  no inbound port). The signed `/__deploy` webhook is retained as an **optional future upgrade**
  only if IBA later exposes a public HTTPS endpoint. (Resolves FR-014's open fork.)
- **Q: Deploy trigger style / downtime?** Auto-deploy on push to `release`; brief restart accepted
  (single binary serves API + frontend, no version skew). (From the 2026-06-25 design session.)

### Session 2026-06-25 (admin portal amendment)

- **Q: How is the admin established + what can the portal do?** A single **admin is env-seeded on
  first boot** (`NOVIQ_ADMIN_USER`/`NOVIQ_ADMIN_PASS`, `role='admin'`). The **admin portal**
  (admin-only) can **create, list, and disable/re-enable** beta-tester accounts. Full RBAC
  (departments, per-doc ACL, password reset, role editing) remains the follow-on. (Adds US7.)

## Overview

Today Noviq is a **monolithic Tauri desktop app**: the SolidJS frontend talks to the Rust backend
through Tauri's in-process IPC (`invoke`), and each install runs its **own bundled PostgreSQL**
(loopback-only) plus its own embedding/LLM models and on-disk caches (`ir_cache/`, `page_cache/`).
Nothing crosses a network.

This feature splits that monolith into a **headless server + thin web client** so multiple users
can use Noviq **over the internet** from a browser, against **one shared corpus**, with the heavy
work (retrieval, embeddings, LLM) running on a **single GPU server** (IBA Karachi cloud box).

The immediate goal is a **pilot**: 2–3 authenticated users testing **RAG / feature quality** over
the internet. The design must also be the **foundation** for the multi-tenant RBAC model already
discussed (super-admin → department admins → users; per-document private/department visibility) —
this feature builds the seam; full RBAC is a follow-on feature.

**Why now:** the target is a headless, internet-facing GPU server. Remote-desktop-to-a-GUI fights
that environment (virtual display, laggy GUI, one shared session, an open port on the internet).
A headless HTTP service + browser client is the natural shape and is reusable, not throwaway.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Use Noviq from a browser, no install (Priority: P1)
A tester opens a URL (e.g. `https://noviq.<host>/`) in any browser on any OS, logs in, and uses
chat / retrieval / drafting against the shared indexed corpus — without installing anything.

**Why P1:** this is the deliverable. Removes "ship an exe per OS" and the cross-platform build
blocker; a browser is the client.

**Acceptance:**
- Opening the served URL in Chrome/Firefox/Edge loads the working app (no `window.__TAURI__`
  dependency anywhere in the runtime path).
- Every backend operation the desktop app supports is reachable over the HTTP API.
- A wrong/blocked operation returns a structured error, never a silent IPC failure.

### User Story 2 - Secure authenticated access over the internet (Priority: P1)
Each tester has a login; unauthenticated requests are rejected; traffic is encrypted; the database
is never exposed to clients.

**Why P1:** internet exposure makes this mandatory. Direct DB/remote-desktop ports on the public
internet are unacceptable.

**Acceptance:**
- All client↔server traffic is over **HTTPS/TLS**.
- Every API endpoint requires a valid session/token; anonymous calls get `401`.
- Passwords stored with a strong KDF (argon2id); no plaintext, no DB credentials on the client.
- Clients talk **only** to the HTTP API — never directly to PostgreSQL (so future ACLs can't be
  bypassed).
- Brute-force protection (rate-limit / lockout) on login.

### User Story 3 - GPU-accelerated responses (Priority: P2)
LLM generation and embedding run on the server GPU, so responses and indexing are materially faster
than the laptop CPU build.

**Why P2:** the GPU is the reason for this box; the LLM-response stage benefits most.

**Acceptance:**
- `llama-cpp-2` built with CUDA (cuBLAS); model layers offloaded to GPU; server logs confirm GPU
  use.
- A representative query's generation latency improves measurably vs the CPU build (record before/
  after).
- Embedding/indexing throughput on the server ≥ the laptop baseline.

### User Story 4 - Concurrent, independent multi-user (Priority: P1)
Two or three testers use the app **at the same time** in their own browser sessions without
colliding (unlike a shared remote-desktop screen).

**Why P1:** the whole point of leaving remote desktop behind.

**Acceptance:**
- N simultaneous sessions each see their own UI state and query results.
- Concurrent reads/queries do not corrupt or block each other.
- Concurrent indexing requests are serialized/queued safely (see FR — indexing ownership).

### User Story 5 - Foundation for department RBAC (Priority: P3, future hook)
The data model and API leave room for the future model (departments, roles, per-document
private/department visibility) without a rewrite.

**Why P3:** not needed for the quality pilot, but the seam must not preclude it.

**Acceptance:**
- `user` identity threads through every API call (so ACL filtering can be added later at the
  retrieval layer without re-plumbing).
- Schema reserves `department_id`, `owner_user_id`, `visibility` on sources (nullable now;
  enforced by the later RBAC feature).

### User Story 6 - Update the running server from local development (Priority: P1)
A developer edits code on the laptop, pushes to GitHub, and the running GPU server picks up the
change — rebuilds (frontend + CUDA backend) and restarts — without a manual SSH build each time,
and without taking the box down for long. See `deployment-and-updates.md` for the full design.

**Why P1:** a pilot is iterative; without a repeatable update path every fix is a manual,
error-prone SSH build. The server is also the only viable CUDA build host, so updates *must* flow
through it.

**Acceptance:**
- Pushing to the designated `release` ref triggers an automatic rebuild + restart on the server;
  pushes to other branches do **not** deploy.
- Backend and frontend always advance together (single binary serves the built web client) — no
  version skew between API and UI.
- A failed post-deploy health check rolls back to the previous working build automatically.
- The deploy survives the server restart it triggers (build runs decoupled from the server
  process), and no secrets or model files are committed to git.

### User Story 7 - Admin portal to manage beta testers (Priority: P2)
An admin logs in and uses an admin-only portal to add new tester accounts (username + temporary
password), see all users, and disable/re-enable accounts — so beta onboarding/offboarding needs no
DB access. The admin is env-seeded on first boot; full RBAC (departments, per-doc ACL, password
reset, role editing) is the follow-on.

**Why P2:** the pilot needs to grow/shrink its tester set without SSH+`psql`. Builds on US2 auth.

**Acceptance:**
- A first **admin** account is seeded from env on first boot; no admin endpoint is reachable without
  an authenticated admin session.
- Admin can **create** a user (username + temp password, role `user`), **list** all users, and
  **disable/re-enable** an account; a disabled user's login is rejected and existing sessions stop
  working.
- Non-admin users get `403` on every admin endpoint; the portal route is hidden/again `403` for them.

### Edge Cases
- Server restart mid-session → clients re-authenticate gracefully; in-flight indexing resumes or
  fails cleanly (reuse existing progress/recovery).
- A client indexes a doc, another queries it → caches (`page_cache`, `ir_cache`) must live on the
  **server**, co-located with the shared DB (the local-cache/shared-DB mismatch that killed the
  "full app + shared DB" option). Server owns all indexing and cache files.
- University network: server reachable only via VPN/jump host → document the access path; HTTPS
  endpoint must work behind whatever IBA IT provides (reverse proxy / port).
- GPU OOM under concurrent LLM load → single model slot + request queue (reuse the existing
  single-`ModelSlot` constraint, Principle III).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (HTTP API seam):** Expose the existing backend operations (currently Tauri commands) as
  an authenticated HTTP/JSON API. The frontend calls the API instead of `invoke`. A single client
  abstraction switches transport (Tauri IPC for legacy desktop, HTTP for web) so the UI code is
  shared.
- **FR-002 (headless server):** The Rust backend runs as a long-lived headless service (no Tauri
  window, no X/display) on Linux, serving the API + static web client.
- **FR-003 (web client):** Build/serve the SolidJS frontend as a static web app over HTTPS; remove
  hard `window.__TAURI__` runtime dependencies from the request path.
- **FR-004 (auth):** User accounts with argon2id password hashing; login issues a session token;
  every API call is authenticated; logout/expiry supported.
- **FR-005 (TLS):** All traffic over HTTPS (TLS termination at the server or a reverse proxy).
- **FR-006 (central DB):** One shared PostgreSQL on the server (reuse the bundled-PG engine or a
  managed PG); clients never connect to it directly.
- **FR-007 (server-owned indexing + caches):** Indexing runs on the server; `ir_cache`/`page_cache`
  live on the server filesystem, co-located with the DB. No client-local cache references.
- **FR-008 (indexing concurrency):** Serialize/queue concurrent index jobs with a bounded worker;
  per-job progress events stream to the requesting client.
- **FR-009 (GPU build):** Server build offloads LLM (and where supported, embeddings) to the GPU;
  graceful CPU fallback if the GPU is unavailable.
- **FR-010 (identity threading):** Every API request carries the authenticated user; the retrieval/
  drafting paths accept (but in this feature do not yet enforce) a user/visibility context — the
  hook for RBAC.
- **FR-011 (audit):** Extend the existing `audit_log` to record user + action for queries, indexing,
  exports (multi-user traceability).
- **FR-012 (parity):** The web client reaches feature parity with the desktop app for the pilot
  scope: chat/retrieval, one-ask drafting, document indexing, viewing indexed docs.
- **FR-013 (server-side build):** The GPU server is the build host (CUDA/cuBLAS compiled where the
  toolkit + driver live); the laptop pushes source only, never binaries.
- **FR-014 (auto-deploy on push):** A push to a designated `release` ref triggers an automatic
  rebuild (frontend `npm run build` + backend `cargo build --release` with the GPU feature) and a
  service restart. Pushes to feature branches do not deploy. **Primary trigger is a server-side
  poll-timer** (`git ls-remote origin/release` vs the deployed commit; no inbound port — required
  because the VM is VPN-gated at a private address, see Clarifications). A signed GitHub webhook
  (`/__deploy`, HMAC-SHA256) is an optional future upgrade if a public HTTPS endpoint is exposed.
- **FR-015 (decoupled deploy):** The build/restart runs in a separate unit (`noviq-deploy.service`)
  that the server only triggers — never as a child of the server process it is about to restart.
- **FR-016 (atomic frontend+backend):** A single binary serves the built static frontend and the
  API, so a deploy advances both together with no API/UI version skew (brief restart accepted).
- **FR-017 (safe rollback):** A failed post-deploy health probe automatically restores the previous
  binary + commit and restarts. DB migrations remain additive so an older binary is safe on a newer
  schema.
- **FR-018 (no artifacts in git):** Models (`*.gguf`) and secrets (`.env`, deploy webhook secret)
  are never committed; the deploy never transfers models (managed out-of-band via rsync).
- **FR-019 (admin bootstrap):** On first boot the server seeds a single admin user from
  `NOVIQ_ADMIN_USER`/`NOVIQ_ADMIN_PASS` (argon2id, `role='admin'`) if no admin exists; idempotent on
  later boots.
- **FR-020 (admin-only authz):** A role guard restricts admin endpoints to sessions whose user has
  `role='admin'`; non-admins get `403`. Threads through the same identity context as FR-010.
- **FR-021 (user management):** Admin endpoints to **create** (`POST /api/admin/users`), **list**
  (`GET /api/admin/users`), and **disable/re-enable** (`POST /api/admin/users/:id/disable|enable`) a
  user. Create sets a temp password; disable sets `users.disabled_at` and the auth layer rejects a
  disabled user's login + invalidates their sessions.
- **FR-022 (admin portal UI):** An admin-only page (create form + user list with disable/enable
  toggles) reachable from the web client; hidden/`403` for non-admins.

### Key Entities

- **user** — `id`, `username`, `password_hash`, `created_at`, `role` (`admin`|`user`, used by US7),
  `disabled_at` (nullable; non-NULL = disabled). Reserved for RBAC follow-on: `department_id`.
- **session** — token, user, expiry.
- **sources** (existing) — reserve nullable `department_id`, `owner_user_id`, `visibility`
  (`private`|`department`) for the RBAC follow-on; unused/!enforced in this feature.
- **audit_log** (existing) — add `user_id`, `action`, `target`.

## Success Criteria *(mandatory)*

### Measurable Outcomes
- **SC-001:** A tester on a different machine, over the internet, logs in via browser and completes
  a full query→answer→citation flow against the shared corpus — zero local install.
- **SC-002:** 3 concurrent browser sessions run independent queries with correct, isolated results.
- **SC-003:** No unauthenticated request reaches any data endpoint (verified by test).
- **SC-004:** PostgreSQL is not reachable from any client host (only the server process connects).
- **SC-005:** GPU build shows measurably lower LLM latency than the CPU baseline (record numbers).
- **SC-006:** Indexing a document on the server makes it queryable by all users (caches resolve
  server-side).
- **SC-007:** A code change pushed to `release` from the laptop is live on the server (rebuilt +
  restarted) with no manual SSH build step; a push to a feature branch does not deploy.
- **SC-008:** A deploy whose new build fails its health check leaves the previous working version
  running (verified rollback).
- **SC-009:** An admin creates a new tester from the portal and that tester logs in and runs a query
  — with zero DB/SSH access by the admin.
- **SC-010:** A disabled tester can no longer log in, and a non-admin receives `403` on every admin
  endpoint (verified by test).

## Assumptions
- IBA server is Linux x86_64 with an NVIDIA GPU + CUDA drivers; reachable over the internet via a
  path IBA IT provides (public IP behind firewall, VPN, or jump host). **To confirm with IBA IT.**
- Pilot trust model is low-stakes (2–3 known users); full department RBAC + per-doc ACL is a
  **separate follow-on feature**, not this one.
- The existing PostgreSQL + pgvector schema and retrieval logic are reused unchanged except for the
  identity/visibility hooks (FR-010).

## Non-Goals (Out of Scope)
- Department/role hierarchy and per-document private/department ACL enforcement (the full RBAC
  model) — designed-for here, **built** in a follow-on feature.
- Cross-department/cross-tenant sharing.
- Native per-OS thin-client installers (the browser is the client; a desktop wrapper is optional
  later).
- Horizontal scaling / multiple GPU nodes / load balancing (single-server pilot).
- Public/anonymous access (authenticated users only).
