# Tasks: Server / Thin-Client Deployment (Internet, GPU-backed)

**Feature dir:** `specs/014-server-thin-client/` · **Work branch:** `015-bureaucrat-chat`
**Inputs:** spec.md, plan.md, research.md, data-model.md, contracts/http-api.md, quickstart.md,
deployment-and-updates.md

**VM facts (locked):** Ubuntu 20.04.6, NVIDIA **A40-12Q 12 GB vGPU, driver 470 → CUDA ≤ 11.4**,
12c/62 GB, build under **`/opt` (not `/home`, 43 GB)**. Trigger = **poll-timer** (VPN-gated, no
webhook). Single **ModelSlot** + queue. **Additive** migration only. Models/secrets **never** in git.

**Two streams (sequenced):** Stream A = HTTP-API seam + headless server (FR-001..FR-012, US1–US5).
Stream B = deploy/update path (FR-013..FR-018, US6) — depends on the `noviq-server` binary +
`/api/health` from Stream A.

> Tests are included only where they protect a contract (auth, rollback) — the repo's acceptance
> suites stay model-independent.

---

## Phase 1: Setup

- [X] T001 Add a `server` Cargo feature + deps (`axum` 0.7, `tower-http` 0.5 ServeDir/cors/limit, `argon2`) and a `cuda` feature gating `llama-cpp-2` cuBLAS, in `src-tauri/Cargo.toml` *(cuda feature pre-existed)*
- [X] T002 [P] Add a second binary target `noviq-server` (headless entrypoint, no Tauri, `required-features=["server"]`) in `src-tauri/Cargo.toml` + `src-tauri/src/bin/noviq-server.rs`
- [X] T003 [P] Create the `api` module skeleton (`mod.rs`, `router.rs`, `auth.rs`, `session.rs`, `health.rs`) under `src-tauri/src/api/` and declare it (`#[cfg(feature="server")]`) in `src-tauri/src/lib.rs` *(static-serving lives in router.rs, no separate static.rs)*
- [X] T004 [P] Add `.env.example` (`NOVIQ_DATA_DIR/WEB_DIR/BIND/GPU_LAYERS`, optional `DEPLOY_SECRET`) at repo root; `.env`/`models/`/`target/`/`node_modules/`/`dist/` already git-ignored

## Phase 2: Foundational (blocking — all stories depend on these)

- [X] T005 Write the **additive** migration creating `users` + `session` and adding nullable `department_id`/`owner_user_id`/`visibility` to `sources` and `user_id`/`action`/`target` to `audit_log`, in `src-tauri/src/db/pg/migrations/005_server_users_sessions.sql` (additive `ADD COLUMN IF NOT EXISTS` — older binary stays valid → rollback-safe)
- [X] T006 Implement the axum app builder in `src-tauri/src/api/router.rs`: mount `/api/*` then `tower-http` `ServeDir` for the Vite `dist/` with SPA fallback to `index.html` (FR-003/FR-016)
- [X] T007 Implement `GET /api/health` → `{status,gpu,layers,version}` in `src-tauri/src/api/health.rs` (unauthenticated; consumed by the deploy probe)
- [X] T008 Wire `noviq-server.rs` to read env, init the pg pool + single `ModelSlot` (lazy, `cfg(llm)`), build the router (T006), and serve on `:8080` headless (no Tauri window/X) (FR-002)
- [X] T009 [P] Frontend transport seam `src/api/transport.ts` *(repo uses `src/api/`, not `src/lib/`)*: `call(cmd,args)` → Tauri `invoke` when `window.__TAURI__` present else `fetch('/api/'+cmd, …Authorization)` (FR-001)

---

## Phase 3: US1 — Use Noviq from a browser, no install (P1) 🎯 MVP

**Goal:** browser opens the served URL, uses chat/retrieval/drafting/indexing over HTTP — no install.
**Independent test:** load the URL in Chrome, run query→answer→citation; every desktop op reachable; errors structured.

> ✅ **T009b DONE — AppHandle made optional through the chat pipeline.** The deep paths
> (`ModelSlot::infer_with_cancel_temp`, `query_router::route_scoped_with_override`) already took
> `Option<AppHandle>`, so the minimal correct seam was to thread `Option<&AppHandle>` through the
> assistant capability layer rather than extract a new trait through the hot path. Desktop passes
> `Some(&app)`; the headless HTTP path passes `None`. Live SSE progress is a later additive
> enhancement (US4); the answer/citations flow works headless now. Both builds compile clean.

- [X] T009b [US1] **AppHandle-optional seam**: `app: &AppHandle` → `app: Option<&AppHandle>` across `commands/assistant.rs` dispatch + `capabilities/{draft,guidance,summarize_doc,compare,conflict,action_items,mod}.rs` + `mapreduce.rs`; `.emit` guarded; `Some(app.clone())`→`app.cloned()`. Desktop (`cargo check --lib`) + server (`--features server`) both exit 0. Unblocks T011/T013.
- [x] T010 [US1] Migrate UI pages to import `transport.call` instead of `invoke` (chat, one-ask, sources, folders) across `src/pages/*` and `src/components/*` *(safe anytime — falls back to invoke in Tauri; deferred until endpoints exist)*
- [X] T011 [US1] `POST /api/query` (one chat turn) wired in `src-tauri/src/api/query.rs` → `assistant::run_turn` (extracted from `send_message`; both transports share it, no logic duplicated). Auth-protected. AppState expanded with embedder/model_slot/reranker_slot/scheduler; bin constructs them. Desktop+server `cargo check` exit 0; **616 lib tests pass**. *(`/api/search` still pending)*
- [x] T012 [P] [US1] Drafting endpoints — **subsumed by T011**: F015 retired the `one_ask`/`drafting` command modules; drafting is now a capability of the unified assistant, reached via `POST /api/query` (intent-routed `draft`). No separate endpoints needed.
- [x] T013 [P] [US1] Map document endpoints `POST /api/sources`, `/api/folders`, read `/api/analysis`,`/api/entities` *(read paths free of AppHandle can land before T009b)*
- [X] T014 [US1] Structured error envelope `{error:{code,message}}` for all handlers in `src-tauri/src/api/mod.rs` (`ApiError`)
- [X] T015 [US1] Vite build serves under the binary — `dist/` is the ServeDir root; vite default `base:"/"` is correct for serving at root (no config change needed)

**Checkpoint:** browser reaches feature parity for read/query/draft against the shared corpus *(pending T009b for streaming paths)*.

---

## Phase 4: US2 — Secure authenticated access (P1)

**Goal:** logins, encrypted traffic, DB never exposed, brute-force protection.
**Independent test:** anonymous call → 401; login issues token; `psql` from a client host fails.

- [X] T016 [US2] argon2id password hashing + verify helpers in `src-tauri/src/api/auth.rs` (`hash_password`/`verify_password`)
- [X] T017 [US2] `POST /api/login` (verify, issue ≈244-bit token stored sha256-hashed in `session`, return `{token,expires_at}`) + `POST /api/logout` (revoke all user sessions) + `GET /api/me` in `src-tauri/src/api/auth.rs`
- [X] T018 [US2] Auth middleware `require_auth` (resolve `Authorization: Bearer`, inject `AuthUser`, reject missing/expired/disabled → 401) + `AuthUser`/`AdminUser` extractors in `src-tauri/src/api/session.rs`; applied to the protected `/api/*` subtree (health+login outside it)
- [X] T019 [P] [US2] Login rate-limit / lockout (per username, `MAX_FAILS=5` / 15-min window) in `src-tauri/src/api/auth.rs` (FR-004)
- [X] T020 [P] [US2] Server binds loopback (`NOVIQ_BIND` default `127.0.0.1:8080`); `deploy/Caddyfile` documents TLS termination so clients never reach PG (FR-005/FR-006, SC-004)
- [~] T021 [US2] Tests: argon2id round-trip + lockout unit GREEN; DB-backed HTTP test **written + compiling** (`src-tauri/tests/api_auth.rs`: wrong-pw→401, login→/me, bad-token→401, logout-revoke, 5-fail lockout→429). **Pending on VM:** run it (`--ignored`, bundled PG).

**Checkpoint:** auth layer complete + compiling + unit-tested; full HTTP/DB integration test runs with PG on the VM.

---

## Phase 5: US4 — Concurrent, independent multi-user (P1)

**Goal:** 2–3 simultaneous sessions, isolated state, safe concurrent indexing.
**Independent test:** 3 sessions run independent queries with correct isolated results; index jobs queue.

- [x] T022 [US4] Per-request session/user context carried through handlers (no shared mutable UI state server-side); verify retrieval is pool-parallel in `src-tauri/src/api/router.rs`
- [x] T023 [US4] Single-`ModelSlot` request **queue** (mpsc + worker) so concurrent LLM/generation serialize without GPU OOM (Principle III, FR-008) in `src-tauri/src/services/` (reuse existing ModelSlot)
- [x] T024 [US4] Indexing concurrency: bounded by existing `indexer::indexing_sem()`; per-file progress broadcast (`indexer::index_progress_channel`) streamed over `GET /api/index/events` (SSE, query-token auth) — replaces the Tauri `source:indexing-progress` channel. Triggers `POST /api/add_sources`/`add_indexed_folder` via extracted `index_sources`/`index_folder` service fns (`app: Option<AppHandle>`). Files: `services/indexer.rs`, `commands/{sources,folders}.rs`, `api/{index_events,documents,router,mod}.rs`
- [x] T025 [P] [US4] Frontend `onIndexProgress` (`src/api/transport.ts`) — Tauri `listen` on desktop, `EventSource('/api/index/events?token=…')` on web; `SourceList.tsx` rewired to it

**Checkpoint:** concurrent use is isolated and indexing is serialized safely.

---

## Phase 6: US3 — GPU-accelerated responses (P2)

**Goal:** LLM (and embeddings where supported) run on the A40 vGPU; measurably faster than CPU.
**Independent test:** `/api/health` shows `gpu:true` + lower generation latency than the CPU baseline.

- [x] T026 [US3] Build `noviq-server` with `--features "server,cuda"`; ensure `llama-cpp-2` cuBLAS links against **CUDA ≤ 11.4** (driver 470 cap) — document toolkit version in `quickstart.md` ✅ 2026-06-27: nvcc 11.4 V11.4.152, driver 470.223.02; `/api/health` gpu:true layers:999; offloaded 33/33 layers to GPU
- [x] T027 [US3] `n_gpu_layers` from `NOVIQ_GPU_LAYERS` env (override auto-sizing; `0` = CPU fallback, FR-009); offload count logged on model load — `services/llm.rs::ModelSlot::load`. Sizing the 12 GB value = a VM tuning step (see quickstart).
- [x] T028 [P] [US3] Record before/after generation latency (CPU vs GPU) for a representative query in `specs/014-server-thin-client/quickstart.md` (SC-005 evidence) ✅ 2026-06-27: GPU 4.30s vs CPU 230.75s (~53×), table in quickstart.md

**Checkpoint:** GPU offload confirmed in logs + latency win recorded.

---

## Phase 7: US6 — Update the running server from local dev (P1) · Stream B

**Goal:** `make deploy` → server poll-timer rebuilds (frontend + CUDA backend) → restart → health probe → auto-rollback.
**Independent test:** push to `release` goes live with no manual SSH build; feature-branch push does not; failed build leaves previous version running.
**Depends on:** T007 (`/api/health`), T008 (binary), T026 (CUDA build).

- [X] T029 [US6] `deploy/deploy.sh` — fetch+checkout `release`, `npm ci && npm run build`, `cargo build --release --features server,cuda`, restart, post-restart `/api/health` probe, **auto-rollback** to previous binary+commit on failure (paths under `/opt/noviq`) (FR-013/FR-017). **+ DEPLOYING sentinel** (touch at start / clear before restart + on exit) driving the "server updating" banner.
- [X] T030 [P] [US6] `deploy/noviq-server.service` (headless service, EnvironmentFile=.env, Restart=on-failure)
- [X] T031 [P] [US6] `deploy/noviq-deploy.service` (oneshot, decoupled — runs deploy.sh OUTSIDE the server process tree so the restart can't kill it) (FR-015)
- [X] T032 [US6] `deploy/noviq-deploy.timer` + `deploy/noviq-deploy-check.service` (+ `deploy/check-release.sh`) — poll `origin/release` every ~2 min; start `noviq-deploy.service` on change (PRIMARY trigger; no inbound port) (FR-014)
- [X] T033 [P] [US6] `deploy/sudoers.noviq` — passwordless `systemctl restart noviq-server` + `systemctl start --no-block noviq-deploy.service` only
- [X] T034 [P] [US6] Root `Makefile` `deploy:` target → `git push origin $(current-branch):release`
- [x] T035 [US6] `deploy/test_deploy_rollback.sh` — runs the real `deploy.sh` against a throwaway repo with stubbed tools; asserts failed probe → rollback to prev commit+binary (SC-008) and healthy probe → live on `release`. **GREEN locally (7/7), no GPU needed.** `deploy.sh` made env-overridable to enable it.
- [~] T036 [P] [US6] **WON'T-DO (2026-06-27)** — optional `POST /__deploy` HMAC webhook is unnecessary; deploy trigger is the `noviq-deploy.timer` poll (no inbound port). Revisit only if a public deploy endpoint is later exposed.

> **NEW (2026-06-26) — "server is updating" notification (browser UX).** `deploy.sh` writes a
> `DEPLOYING` sentinel under `NOVIQ_DATA_DIR`; `/api/health` reports `status:"deploying"` while it
> exists. Frontend `src/stores/serverStatus.ts` polls `/api/health` (web transport only) and
> `src/components/ServerStatusBanner.tsx` shows an amber "Noviq is updating…" banner during the
> build AND the restart blip (unreachable = treated as updating), then auto-reloads when a new
> `version` comes back. Wired in `App.tsx`. Both builds compile; `tsc` clean.

**Checkpoint:** `make deploy` updates the live server unattended within a poll cycle; rollback verified.

---

## Phase 8: US5 — Foundation for department RBAC (P3, hooks only)

**Goal:** identity + reserved columns thread through; no enforcement yet.

- [x] T037 [US5] Thread `user_id` into retrieval/drafting/indexing service-call context (no filtering yet) — the seam for the RBAC follow-on (FR-010) in `src-tauri/src/api/router.rs` + service signatures
- [x] T038 [P] [US5] Extend `audit_log` writes with `user_id`/`action`/`target` for query/index/export/login in `src-tauri/src/commands/audit.rs` (FR-011)

---

## Phase 8b: US7 — Admin portal to manage beta testers (P2) · depends on US2

**Goal:** env-seeded admin creates/lists/disables tester accounts from a portal — no DB/SSH.
**Independent test:** admin creates a tester who then logs in; disabled tester can't log in; non-admin → 403.
**Depends on:** US2 (T016–T019 auth/session) + the migration 005 `role`/`disabled_at` columns (T005, done).

- [x] T043 [US7] Admin bootstrap (FR-019): on first boot seed one `role='admin'` user from `NOVIQ_ADMIN_USER`/`NOVIQ_ADMIN_PASS` if no admin exists (idempotent), in `src-tauri/src/bin/noviq-server.rs` + a helper in `src-tauri/src/api/auth.rs`
- [x] T044 [US7] Role guard (FR-020): admin-only tower layer/extractor rejecting non-`admin` sessions with `403`, in `src-tauri/src/api/session.rs`
- [x] T045 [US7] Admin endpoints (FR-021): `GET/POST /api/admin/users`, `POST /api/admin/users/:id/{disable,enable}` (create=temp password; disable sets `disabled_at` + revokes sessions), in `src-tauri/src/api/admin.rs` (new) wired in `router.rs`
- [x] T046 [US7] Enforce `disabled_at` in the login + session-resolve paths (reject disabled user, drop their sessions), in `src-tauri/src/api/auth.rs` + `session.rs`
- [x] T047 [P] [US7] Admin portal page (FR-022): create form + user list with disable/enable toggles, admin-only route hidden/403 for non-admins, in `src/pages/Admin.tsx` (+ route) using `transport.call`
- [x] T048 [US7] Test: non-admin → 403 on every admin endpoint; disabled user login rejected (SC-010), in `src-tauri/tests/api_admin.rs`

## Phase 9: Polish & Cross-Cutting

- [x] T049 [P] CORS/security headers + payload size limits on `/api/*` in `src-tauri/src/api/router.rs`
- [x] T050 [P] Update `CHANGELOG_DEV.md` with the server-split + deploy-path landing (paths + reasoning)
- [ ] T051 [P] Finalize `quickstart.md` runbook against the real VM (CUDA 11.4 toolkit, `/opt` paths, Caddy) and check off SC-001..010
- [ ] T052 Acceptance gate: SC-001 (browser full flow), SC-002 (3 concurrent), SC-003 (401), SC-004 (PG unreachable), SC-005 (GPU latency), SC-006 (index→queryable), SC-007 (release deploy live), SC-008 (rollback), SC-009 (admin creates tester→login), SC-010 (disabled login rejected + non-admin 403)

---

## Dependencies & order

- **Setup (T001–T004 ✅)** → **Foundational (T005–T009 ✅)** block everything.
- **Stream A:** US1 (T010–T015) → US2 (T016–T021) → US4 (T022–T025) → US3 (T026–T028). US1 is the MVP.
- **Stream B:** US6 (T029–T036) depends on T007+T008+T026.
- **US7 admin portal (T043–T048)** depends on US2 (auth/session).
- US5 (T037–T038) can land any time after Foundational (it's additive hooks).
- Polish (T049–T052) last.

## Parallel opportunities

- T002/T003/T004 in parallel (different files).
- Within US1: T012/T013 parallel; within US2: T019/T020 parallel.
- Within US6: T030/T031/T033/T034 (separate deploy files) parallel; T029→T032→T035 sequential.

## MVP scope

**US1 + Foundational** = a logged-out browser-served app reaching query/draft parity. Add **US2**
(auth) to make it safe for the pilot, then **US6** (deploy) so iteration no longer needs manual SSH.
</content>
