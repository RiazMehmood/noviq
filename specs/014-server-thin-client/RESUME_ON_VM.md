# ▶ Resume on the GPU VM — F014 finish line

Written 2026-06-26. Everything that can be done **without** the GPU is already coded,
compiled, and (where possible) tested locally. This is the ordered, copy-paste worklist
for the VM session. Full standup steps live in `quickstart.md`; this is the short path.

## Before you start (from the Windows machine)
- Easy Connect VPN up (Windows — local Linux EasyConnect GUI crashes on 22.04).
- `ssh` into the VM (creds `~/Desktop/sudo`). VM = Ubuntu 20.04, private IP ~10.150.x.100.
- Branch to deploy = whatever you `make deploy`; current work branch is `014-server-thin-client`.
- Guide: `D:\GPU_VM_Setup_Guide.md`. Build under `/opt/noviq` (43 GB; not /home).

## What's already DONE (no VM action) ✅
- US1 chat + document/KG read endpoints, transport seam, gen-gate, CORS/limits/headers.
- US7 admin portal (endpoints + page + guard + seed + disabled enforcement).
- US4 indexing concurrency + `GET /api/index/events` SSE + frontend `EventSource`.
- **T027** `NOVIQ_GPU_LAYERS` env wiring + CPU fallback (`services/llm.rs`).
- **T021/T048** DB-backed HTTP tests **written + compiling** (`tests/api_auth.rs`, `tests/api_admin.rs`).
- **T035** `deploy/test_deploy_rollback.sh` — **GREEN locally (7/7)**.
- `quickstart.md` runbook + `CHANGELOG_DEV.md` up to date.

## What's LEFT on the VM (small) — do in this order

### 1. Stand up (one-time) — `quickstart.md` §0–§4
`nvidia-smi` → clone → `.env` (set `NOVIQ_GPU_LAYERS`, admin creds) → rsync models →
`npm ci && npm run build` → **T026** `cargo build --release --features "server,cuda"`
(ensure cuBLAS links **CUDA ≤ 11.4**, driver 470 cap — note the toolkit version in quickstart) →
install systemd units → `curl /api/health`.

### 2. Run the server-side tests (bundled PG)
```bash
cd /opt/noviq/echoAI/src-tauri
POSTGRESQL_VERSION="=17.10.0" cargo test --features server \
  --test api_auth --test api_admin -- --ignored --nocapture
```
Green here closes **T021** and the test half of **T048** (SC-003/009/010).

### 3. T028 — GPU vs CPU latency (SC-005)
Run the `time curl … /api/query` bench from `quickstart.md` twice (`NOVIQ_GPU_LAYERS=0`
then your 12 GB value); paste the two numbers into the quickstart table. Confirm
`/api/health` shows `gpu:true`.

### 4. T035 sanity (already green locally) + first live deploy (SC-007)
```bash
bash deploy/test_deploy_rollback.sh        # re-confirm on the VM
# from the laptop:
make deploy                                # push current branch -> origin/release
journalctl -u noviq-deploy.service -f      # watch poll-timer build → restart → probe
```

### 5. T052 — acceptance gate, tick SC-001..010 (`quickstart.md` smoke list)
Browser-on-VPN full flow (SC-001), 3 concurrent (SC-002), unauth→401 (SC-003),
PG unreachable from a client (SC-004), GPU latency (SC-005), index→queryable (SC-006),
release deploy live / feature-push doesn't (SC-007), rollback (SC-008),
admin creates tester→login (SC-009), disabled→401 + non-admin→403 (SC-010).

### 6. T051 — finalize the runbook
Fill the real CUDA toolkit version, `/opt` paths, and access method (ngrok/Caddy) into
`quickstart.md`; check off the SC list.

## Access decision (locked 2026-06-26)
ngrok free static domain fronting the **authed** server (poll-timer deploy unchanged);
brand `noviq.<domain>` later. Auth must be solid before the tunnel is public — heads-up
to IBA IT. Deviates from ADR-0017 VPN-only.

## Gotchas
- `EventSource` (SSE) can't send headers — the web client passes the bearer token as
  `?token=` to `/api/index/events`; that route is intentionally outside the auth middleware
  and self-checks the token. Don't "fix" it by moving it under `require_auth`.
- `NOVIQ_GPU_LAYERS=0` is the CPU fallback switch; unset = desktop auto-calc.
- Additive migrations apply on server startup; a failed deploy rolls back binary **and** commit.
