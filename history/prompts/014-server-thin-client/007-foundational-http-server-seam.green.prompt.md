---
id: 007
title: Foundational HTTP server seam T001-T009
stage: green
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.implement
labels: [implement, axum, server, foundational]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr/0017-server-deployment-and-update-strategy.md
  pr:
files:
  - src-tauri/Cargo.toml
  - src-tauri/src/bin/noviq-server.rs
  - src-tauri/src/api/mod.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/api/health.rs
  - src-tauri/src/api/auth.rs
  - src-tauri/src/api/session.rs
  - src-tauri/src/lib.rs
  - src-tauri/src/db/pg/migrations/005_server_users_sessions.sql
  - src/api/transport.ts
  - .env.example
tests:
  - "cargo check --features server --bin noviq-server: exit 0 (2 pre-existing warnings)"
  - "tsc --noEmit: clean"
---

## Prompt

start /sp.implement with T001–T009

(Feature 014: implement ONLY T001–T009, the local-safe foundation — Cargo features, noviq-server
bin, api/ skeleton, .env.example, additive migration, axum router + ServeDir, /api/health,
headless binary, transport.ts. No CUDA or deploy-stream tasks. Don't clobber 015 artifacts. Verify
with cargo check / tsc.)

## Response snapshot

Implemented all 9 foundational tasks, gated behind a new `server` Cargo feature so the desktop
build is untouched. Backend: `[[bin]] noviq-server` (required-features=server) bootstrapping
tier→init_db(bundled PG)→lazy ModelSlot(cfg llm)→axum::serve; new `api/` module
(AppState + ApiError envelope, router with /api/* + ServeDir SPA fallback, /api/health,
auth/session skeletons); additive migration 005 (users/session + reserved RBAC + audit cols, all
ADD COLUMN IF NOT EXISTS → rollback-safe). Frontend: src/api/transport.ts call() seam (Tauri invoke
vs HTTP fetch + bearer). Verified: `cargo check --features server --bin noviq-server` exit 0 (only
2 pre-existing unrelated dead-code warnings), `tsc --noEmit` clean. Deviations from plan, noted:
transport at src/api/ (repo's real layout, not src/lib/); static-serving folded into router.rs (no
separate static.rs). Marked T001–T009 [X] in tasks.md; logged CHANGELOG.

## Outcome

- ✅ Impact: 014 has a compiling headless-server foundation; US1+ endpoints and the deploy stream can build on it.
- 🧪 Tests: cargo check (server bin) exit 0; tsc clean. No runtime/integration run (PG boot deferred to VM).
- 📁 Files: 11 (Cargo.toml, bin, 5 api/*, lib.rs, migration, transport.ts, .env.example).
- 🔁 Next prompts: US1 (T010–T015) endpoint mapping + call-site migration; on VM: CUDA build (T026/27) + deploy stream (T029–T036). NEW: admin user-management portal request to fold into spec.
- 🧠 Reflection: feature-gating the whole server module keeps the desktop build risk-free and lets the foundation compile without touching live Tauri wiring.

## Evaluation notes (flywheel)

- Failure modes observed: none — clean compile first pass after matching tower-http 0.5 to axum 0.7 (http 1.0).
- Graders run and results (PASS/FAIL): cargo check PASS, tsc PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): boot noviq-server locally against a scratch NOVIQ_DATA_DIR to smoke `/api/health` before wiring US1 endpoints.
</content>
