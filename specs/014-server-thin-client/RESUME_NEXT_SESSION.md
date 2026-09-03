# F014 Resume Plan — server / thin-client

> Written 2026-06-26. Branch `014-server-thin-client`. Read `CHANGELOG_DEV.md`
> first. Status of every task is in `tasks.md`. This file is the ordered
> next-session worklist with the exact seams to follow.

## Established pattern (follow it)
- Endpoints live in `src-tauri/src/api/<name>.rs`, mounted in `api/router.rs`
  inside the **protected** subtree (after `route_layer(require_auth)`), and use
  the `AuthUser` extractor + `ApiError`/`ApiResult` envelope from `api/mod.rs`.
- One implementation, two transports: extract the orchestration into a
  service fn taking `app: Option<&AppHandle>`, then the Tauri command and the
  HTTP handler both call it (pass `Some(&app)` / `None`). Reference: T011 —
  `services/assistant/mod.rs::run_turn` + `api/query.rs`.
- `AppState` (in `api/mod.rs`) holds the shared singletons; the bin
  `src-tauri/src/bin/noviq-server.rs` constructs them at boot.
- Verify gate every change: `cargo check --lib` (desktop) **and**
  `cargo check --features server --bin noviq-server` both exit 0; `tsc --noEmit`
  for frontend; relevant `cargo test --features server --lib`.

## Locally doable (NO VM) — recommended order

### 1. T037 + T038 — US5 RBAC hooks (smallest, additive) ⭐ start here
- T037: thread `user_id` (from `AuthUser`) into retrieval/drafting/indexing
  service-call context — **no filtering yet**, just the seam. Touch
  `api/router.rs` + the service fn signatures you wire endpoints to.
- T038: extend `audit_log` writes with `user_id`/`action`/`target` for
  query/index/export/login in `commands/audit.rs` (migration 005 already added
  the columns).
- Low blast radius, compiles fast, unblocks RBAC follow-on.

### 2. T012 — drafting endpoints (US1)
- Map `POST /api/one_ask`, `/api/drafting/*` to existing service fns in
  `services/drafting/` + `services/drafting/one_ask/`. Source commands:
  `commands/one_ask.rs`, `commands/drafting.rs`. Extract any AppHandle-taking
  orchestration to `Option<&AppHandle>` (most deep paths already do — see T009b
  note in CHANGELOG). New file `api/drafting.rs`, mount in `router.rs`.

### 3. T013 — document endpoints (US1)
- `POST /api/sources`, `/api/folders`; read `/api/analysis`, `/api/entities`.
  Source commands: `commands/{sources,folders,analysis,entities}.rs`. Read paths
  are AppHandle-free and can land first.

### 4. T010 — frontend transport migration
- Replace `invoke(...)` with `transport.call(...)` (already exists at
  `src/api/transport.ts`) across `src/pages/*` + `src/components/*` for chat,
  one-ask, sources, folders. Safe in Tauri (falls back to `invoke`). Needs `tsc`.

### 5. US7 admin portal — T043–T048 (depends on US2, which is done)
- T043 admin bootstrap (`seed_admin_if_configured` already exists in
  `api/auth.rs` — just confirm it's called from the bin on boot).
- T044 `AdminUser` 403 guard already exists in `api/session.rs` — verify.
- T045 new `api/admin.rs`: `GET/POST /api/admin/users`,
  `POST /api/admin/users/:id/{disable,enable}`.
- T046 enforce `disabled_at` in login + session-resolve (auth.rs already checks
  `disabled_at` per CHANGELOG — verify, add to login path).
- T047 `src/pages/Admin.tsx` portal via `transport.call`.
- T048 test `src-tauri/tests/api_admin.rs` (non-admin→403, disabled→login reject).

### 6. T022–T025 — US4 concurrency
- T023 single-`ModelSlot` mpsc queue + worker (serialize generation, no GPU OOM).
- T024 indexing job queue + `GET /api/index/events` SSE (replaces Tauri
  `importer://progress`). T025 frontend `EventSource`.

## Blocked on the GPU VM (do on the VM)
- T021 — DB-backed HTTP auth test (`src-tauri/tests/api_auth.rs`) needs bundled-PG harness.
- T026–T028 — `--features server,cuda` build (CUDA ≤ 11.4, driver 470), `n_gpu_layers`
  sizing for 12 GB, CPU fallback, before/after latency in `quickstart.md`.
- T035 — deploy.sh rollback / `release`-only-trigger integration test.

## Polish (last) — T049 CORS/size limits, T050 CHANGELOG, T051 quickstart runbook, T052 acceptance gate SC-001..010.

## VM connect (from MEMORY)
IBA GPU VM, Ubuntu 20.04, SSH via Easy Connect VPN (use Windows for VPN — local
EasyConnect GUI crashes on 22.04). Guide `D:\GPU_VM_Setup_Guide.md`. Build under
`/opt/noviq` (not /home, 43 GB). Branch pushed; `make deploy` pushes
current-branch→`origin/release`.
