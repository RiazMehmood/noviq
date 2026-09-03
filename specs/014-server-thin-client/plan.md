# Implementation Plan: Server / Thin-Client Deployment (Internet, GPU-backed)

**Branch**: `015-bureaucrat-chat` (work branch; feature dir = `014-server-thin-client`) | **Date**: 2026-06-25 | **Spec**: [spec.md](./spec.md)
**Input**: `specs/014-server-thin-client/spec.md` + design seed `deployment-and-updates.md`

> Note: this plan was authored on the active `015-bureaucrat-chat` branch (the deploy work rides on
> top of the 015 chat backend). `setup-plan.sh` keys plan paths off the branch and tried to clobber
> `specs/015/plan.md`; that was restored and this plan written directly to the 014 dir instead.

## Summary

Split the Tauri monolith into a **headless axum server** (one binary that serves an authenticated
HTTP/JSON API **and** the built static SolidJS frontend, talks to the server-local PostgreSQL,
owns indexing + caches, offloads LLM/embeddings to the GPU) plus a **browser thin-client**, and
give it a **repeatable update path**: edit locally → `make deploy` (push to `release`) → a
server-side **poll-timer** rebuilds (frontend + CUDA backend) in a **decoupled** systemd unit →
restart → health probe → **auto-rollback** on failure. The VM is VPN-gated at a private address,
so poll (not webhook) is the trigger.

Two work streams, sequenced: **(A) HTTP-API seam + headless server (FR-001..FR-012)** is the
prerequisite; **(B) deploy/update path (FR-013..FR-018)** sits on top and needs the `noviq-server`
binary + `/api/health` to exist first.

## Technical Context

**Language/Version**: Rust 1.87 (workspace), TypeScript + SolidJS frontend.
**Primary Dependencies**: NEW `axum` 0.7 + `tower`/`tower-http` (HTTP API + static file serving),
`tokio` (already), `argon2` (password KDF), `tower-governor` or hand-rolled limiter (login rate
limit), `cookie`/bearer tokens for sessions; existing `sqlx` 0.8 (postgres), `pgvector`,
`fastembed` 4, `llama-cpp-2` 0.1.143 (CUDA/cuBLAS feature on the server build).
**Storage**: server-local **PostgreSQL 17 + pgvector** (existing, F011); caches `ir_cache/`,
`page_cache/` server-local; `models/*.gguf` server-local (out-of-band, not in git).
**Testing**: `cargo test` (API handler unit + auth + a deploy-script dry-run/integration), `vitest`
(frontend transport abstraction), existing acceptance suites unaffected.
**Target Platform**: Linux x86_64 + NVIDIA GPU (IBA VM, Ubuntu 20.04), headless (no X). Reached via
Easy Connect VPN at a private IP (~10.150.x.100).
**Project Type**: web (backend server crate + static frontend) — new shape vs current monolith.
**Performance Goals**: GPU LLM latency measurably below CPU baseline (SC-005); 3 concurrent
sessions isolated (SC-002); deploy latency ≤ poll interval (≤2 min) + build time.
**Constraints**: single `ModelSlot` + request queue under concurrent load (Principle III); brief
restart acceptable; CUDA must compile on the server (it is the build host); migrations additive.
**Scale/Scope**: pilot = 2–3 authenticated users, single GPU server, no horizontal scaling.

**Open unknowns (→ research.md):** GPU presence/CUDA driver on the VM (`nvidia-smi` not yet run);
exact axum session strategy (opaque DB-backed token vs signed cookie); whether a public reverse
proxy exists for browser clients or testers also use the VPN.

## Constitution Check

*GATE: must pass before Phase 0. Re-check after Phase 1.*

| Principle | Status | Note |
|-----------|--------|------|
| I. Privacy-First, Offline-Always (NON-NEGOTIABLE) | ⚠ Tension → justified | Moves from offline desktop to a private, VPN-only, authenticated server. No public/anonymous access (Non-Goal); data stays on the IBA box; outbound LLM already permitted for the legal domain. See Complexity Tracking. |
| III. Single Model Slot — Bounded RAM (NON-NEGOTIABLE) | ✅ Preserved | Reuse the single `ModelSlot`; concurrent LLM requests serialize through one slot + queue (spec edge case + FR-008). |
| IV. SQLite as Single Source of Truth | ⚠ Pre-existing divergence | Project already on PostgreSQL (F011/F012), pending constitution amendment + merge gate. 014 continues PG; does not introduce a NEW divergence. |
| V. Pending Actions Queue — Safety Gate | ✅ N/A here | Deploy is a separate ops path; no change to the app's action-gate. |
| XI. Citation Discipline (NON-NEGOTIABLE) | ✅ Untouched | Deploy/transport changes do not alter retrieval or citation logic. |
| VIII. Incremental Feature-App Delivery | ✅ Aligned | Stream A then B; each independently testable. |

**Verdict**: PASS with two tracked items (Principle I server tension; Principle IV pre-existing PG
divergence). Both recorded in Complexity Tracking; neither is a new unjustified violation.

## Project Structure

### Documentation (this feature)
```text
specs/014-server-thin-client/
├── spec.md                     # amended (US1–US6, FR-001..FR-018, SC-001..008)
├── deployment-and-updates.md   # design seed for stream B (scripts + units)
├── plan.md                     # this file
├── research.md                 # Phase 0
├── data-model.md               # Phase 1 (user/session/audit + reserved RBAC cols)
├── contracts/                  # Phase 1 (HTTP API surface)
├── quickstart.md               # Phase 1 (stand-up + deploy runbook)
└── checklists/requirements.md  # spec-quality (PASS)
```

### Source Code (repository root)
```text
src-tauri/                      # existing crate (echo_ai_lib) — services/db/commands REUSED
├── src/
│   ├── services/               # retrieval, drafting, indexing — unchanged, called by API
│   ├── db/                     # pg migrations (additive: users, sessions, audit cols)
│   ├── commands/               # existing Tauri commands → thin wrappers over service fns
│   └── api/                    # NEW: axum router, handlers, auth, session, static-serve
│       ├── mod.rs  router.rs  auth.rs  session.rs  static.rs  health.rs
│   └── bin/noviq-server.rs     # NEW: headless binary entrypoint (no Tauri window)
├── Cargo.toml                  # NEW features: `server` (axum stack), `cuda` (llama cuBLAS)

src/                            # SolidJS frontend
├── lib/transport.ts            # NEW: client abstraction — Tauri invoke | HTTP fetch
└── ...                         # pages call transport, not invoke directly

deploy/                         # NEW (stream B) — from deployment-and-updates.md
├── deploy.sh  noviq-server.service  noviq-deploy.service
├── noviq-deploy.timer  noviq-deploy-check.service  sudoers.noviq
Makefile                        # NEW: `make deploy` → push current branch to release
```

**Structure Decision**: keep the single `echo_ai_lib` crate; add an `api/` module + a second binary
target `noviq-server` (headless) alongside the existing Tauri `main`. The web frontend and the Tauri
desktop share UI code via a `transport.ts` seam (FR-001). This is the smallest-diff path —
no workspace split, services reused verbatim.

## Phases

- **Phase 0 (research.md):** resolve GPU/CUDA driver on VM, session strategy, reverse-proxy/TLS
  termination, llama-cpp-2 CUDA feature wiring, axum static-serving of the Vite `dist/`.
- **Phase 1 (data-model.md, contracts/, quickstart.md):** users/sessions/audit schema (additive
  migration) + reserved RBAC columns; HTTP API contract mirroring existing Tauri commands +
  `/api/health` + auth endpoints; quickstart runbook for stand-up + first deploy.
- **Phase 2 (/sp.tasks):** dependency-ordered tasks — Stream A (transport seam → axum router →
  auth/session → static serve → headless binary → CUDA feature → concurrency/queue) then Stream B
  (systemd units → deploy.sh → poll-timer → rollback → Makefile), each with acceptance checks.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Principle I: networked, multi-user server (vs offline-always desktop) | The deliverable is internet/VPN browser access on a shared GPU box for a pilot; a desktop-only app can't serve 2–3 remote testers without per-OS installs | Remote-desktop-to-GUI was evaluated (Option B) and rejected: virtual display, lag, single shared session, open port. Private-VPN + auth keeps the privacy posture. |
| Principle IV: PostgreSQL (not SQLite) | Pre-existing since F011 (pgvector retrieval); 014 reuses it | Reverting to SQLite is out of scope and unrelated to this feature; amendment + merge gate already tracked under F011. |
| Second binary + axum stack added to the crate | Headless service must run without Tauri/X and serve HTTP | A separate workspace crate duplicates wiring; reusing the crate with a new bin target is the smaller diff. |
</content>
