# ADR-0017: Server Deployment and Update Strategy

> **Scope**: The deployment + update model for Feature 014 (server / thin-client). Clusters
> build-host, deploy trigger, artifact shape, restart/rollback, and migration safety — they
> work together and change together.

- **Status:** Accepted
- **Date:** 2026-06-25
- **Feature:** 014-server-thin-client
- **Context:** Noviq is moving from a monolithic Tauri desktop app to a headless Rust server +
  browser thin-client on the IBA Karachi GPU VM (NVIDIA A40-12Q 12 GB, driver 470 → CUDA ≤ 11.4;
  Ubuntu 20.04; `/home` only 43 GB, `/` 148 GB). The VM is reachable **only through the Easy
  Connect VPN at a private address** (~10.150.x.100) — no inbound path from the public internet.
  The team iterates on a laptop and needs code changes to reach the running server **without a
  manual SSH build each time**. `llama-cpp-2` must be compiled with CUDA where the toolkit+driver
  live. A 2–3 user pilot tolerates a brief restart but not a broken server.

## Decision

An integrated deployment + update model:

- **Build host = the GPU server.** The CUDA backend (`cargo build --release --features server,cuda`,
  CUDA ≤ 11.4) and the frontend (`npm run build`) are built **on the VM**. The laptop pushes source
  only; no binaries are shipped. Clone/`target/`/models live under `/opt` (on `/`, 148 GB), never
  `/home`.
- **Trigger = server-side poll-timer on a `release` branch.** `noviq-deploy.timer` polls
  `origin/release` (~2 min); on change it starts the build. Day-to-day pushes to feature branches do
  **not** deploy — `make deploy` promotes the current branch to `release`. (A signed GitHub webhook
  is deferred; impossible while the VM is VPN-private.)
- **Artifact = one binary serving API + static frontend.** `noviq-server` serves `/api/*` and the
  Vite `dist/` (ServeDir + SPA fallback), so backend and frontend always advance together — **no
  API/UI version skew**. A brief `systemctl restart` is accepted.
- **Decoupled build/restart.** The trigger only *starts* a separate `noviq-deploy.service`; the
  build + `systemctl restart noviq-server` run **outside** the server's process tree so the restart
  cannot SIGKILL the in-flight deploy.
- **Auto-rollback on health failure.** After restart, `deploy.sh` probes `/api/health`; on failure it
  restores the previous binary + git commit and restarts.
- **Additive migrations only.** Schema changes never break `TARGET_SCHEMA_VERSION`, so an older
  binary stays valid on a newer schema — rollback is safe.
- **No artifacts in git.** `models/*.gguf` (out-of-band rsync) and secrets (`.env` via systemd
  `EnvironmentFile`) are never committed.

## Consequences

### Positive

- Single command (`make deploy`) ships a vetted change with zero manual SSH building.
- No version skew between UI and API; one artifact to reason about.
- Works behind the VPN with no inbound port or exposed webhook endpoint.
- Failed builds never leave a broken server (auto-rollback + additive-migration safety).
- CUDA correctness guaranteed (compiled against the actual driver/toolkit on the box).

### Negative

- Deploys lag a push by up to one poll interval (~2 min) — acceptable for a pilot, not instant.
- Each `release` promotion triggers a full CUDA `cargo build` (minutes); mitigated by deliberate
  promotion + cargo incremental cache.
- Brief restart drops in-flight requests (no zero-downtime); acceptable at 2–3 users.
- Requires a tightly-scoped passwordless sudoers entry for two `systemctl` actions.
- The GPU box is a single build+serve host (no separation of build and runtime).

## Alternatives Considered

- **GitHub push webhook trigger** — instant, hands-off. Rejected for now: the VM is VPN-private, so
  GitHub cannot reach it inbound. Retained as a future upgrade if a public HTTPS endpoint appears.
- **Manual SSH `./deploy.sh`** — simplest. Rejected: defeats the core "no manual build per change"
  goal of US6.
- **Build artifacts in CI and ship binaries** — avoids building on the GPU box. Rejected:
  cross-compiling `llama-cpp-2` cuBLAS against the VM's exact 470/CUDA-11.4 driver from CI is
  impractical and fragile; the box is the only correct CUDA build host.
- **Zero-downtime blue-green (two binaries + proxy switch)** — no dropped requests. Rejected as
  overkill for 2–3 users; the seam is noted for later but not built.
- **Separate workspace crate for the server** — cleaner module boundary. Rejected: reusing the
  existing `echo_ai_lib` crate with a new `noviq-server` bin target is the smaller diff and reuses
  services verbatim.

## References

- Feature Spec: specs/014-server-thin-client/spec.md (US6, FR-013…FR-018, SC-007/008)
- Implementation Plan: specs/014-server-thin-client/plan.md
- Design seed: specs/014-server-thin-client/deployment-and-updates.md
- Research: specs/014-server-thin-client/research.md (R1 trigger, R2 GPU/CUDA/disk)
- Related ADRs: ADR-0015 (PostgreSQL as single source of truth — the DB this server fronts),
  ADR-0014 (database portability seam), ADR-0016 (unified bureaucrat chat assistant)
- Evaluator Evidence: history/prompts/014-server-thin-client/004-server-thin-client-plan.plan.prompt.md
</content>
