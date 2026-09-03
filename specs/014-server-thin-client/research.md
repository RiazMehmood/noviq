# Phase 0 Research — Server / Thin-Client Deployment

## R1. Deploy trigger: poll-timer vs webhook
- **Decision:** server-side **poll-timer** primary (`git ls-remote`/`fetch` of `origin/release`
  every ~2 min via `noviq-deploy.timer`).
- **Rationale:** VM is VPN-gated at a private address (~10.150.x.100, confirmed 2026-06-25 via
  Windows VPN→SSH). GitHub cannot reach a private IP, so a push webhook is impossible without a
  public reverse proxy. Poll needs no inbound port.
- **Alternatives:** signed `/__deploy` webhook (deferred — only viable if IBA exposes a public
  HTTPS endpoint); manual SSH `./deploy.sh` (rejected — defeats the "no manual build" goal).

## R2. GPU / CUDA on the VM — CONFIRMED 2026-06-25
- **VM specs:** Ubuntu 20.04.6 LTS (`yshaikh-vm`); **GPU NVIDIA A40-12Q, 12 GB VRAM (vGPU slice),
  driver 470.223.02**; 12 cores / 62 GB RAM; disk `/` 148 GB free, **`/home` only 43 GB free**.
- **Decision:** build `llama-cpp-2` with the CUDA (cuBLAS) feature **on the server**.
  - **CUDA toolkit ≤ 11.4** — the 470.x driver branch caps at CUDA 11.4; the GPU build must target
    CUDA 11.4 (or use a 11.4-compatible toolkit), else the build/runtime will mismatch the driver.
  - **12 GB VRAM** is the offload budget — fine for the small generator (0.5B) with full offload;
    larger models cap `n_gpu_layers` to fit. Keep the single-ModelSlot queue (R7).
  - **CPU fallback** retained (FR-009) if the vGPU slice is unavailable.
- **Disk placement (hard constraint):** clone, `cargo target/`, `node_modules`, and `models/` must
  live under **`/opt` (on `/`, 148 GB)** — NOT `/home` (43 GB, too small for target+models).
  Quickstart already uses `/opt/noviq`; lock it in.
- **Rationale:** the toolkit + driver only exist on the GPU box → server is the only build host.

## R3. Session / auth strategy
- **Decision:** opaque **DB-backed session token** (random 256-bit, stored hashed in `session`
  table, sent as `Authorization: Bearer` + httpOnly cookie), `argon2id` for password hashing,
  expiry + logout-revocation, login rate-limit/lockout.
- **Rationale:** revocable, simple to reason about for 2–3 users, no signing-key management;
  threads `user_id` into every request for the RBAC hook (FR-010).
- **Alternatives:** stateless JWT (rejected — revocation is awkward, overkill for the pilot).

## R4. TLS termination
- **Decision:** terminate TLS at a **reverse proxy (Caddy)** in front of `noviq-server:8080`
  (automatic certs if a public name exists; otherwise self-signed/internal CA on the VPN).
- **Rationale:** keeps the Rust process simple; Caddy is one binary, auto-HTTPS. rustls in-process
  is the fallback if a proxy isn't permitted.
- **Open:** whether testers reach the box only via VPN or via an IBA-provided public proxy — affects
  cert strategy, not the API design.

## R5. axum serving the SolidJS build
- **Decision:** `tower-http::services::ServeDir` serves the Vite `dist/` with SPA fallback to
  `index.html`; `/api/*` mounted before the static fallback. Single binary, single port → no
  API/UI version skew (FR-016).
- **Rationale:** simplest single-artifact deploy; brief restart swaps both at once.

## R6. Transport seam (shared UI for Tauri + web)
- **Decision:** `src/lib/transport.ts` exposes `call(cmd, args)`; picks Tauri `invoke` when
  `window.__TAURI__` is present, else `fetch('/api/'+cmd)`. Pages stop importing `invoke` directly.
- **Rationale:** one UI codebase for legacy desktop + web (FR-001/FR-003); incremental migration.

## R7. Concurrency under one ModelSlot
- **Decision:** reuse the single `ModelSlot`; serialize LLM/generation through an async queue
  (mpsc + worker) so concurrent requests don't OOM the GPU (Principle III, FR-008). Reads/retrieval
  parallel via the sqlx pool.
- **Rationale:** matches the constitution's bounded-RAM single-slot invariant.

## R8. Migrations
- **Decision:** one **additive** migration adds `users`, `session` tables and nullable
  `department_id`/`owner_user_id`/`visibility` on `sources` + `user_id`/`action`/`target` on
  `audit_log`. No `TARGET_SCHEMA_VERSION`-breaking change → older binary safe on newer schema
  (supports rollback, FR-017).
</content>
