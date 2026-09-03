---
id: 012
title: Deploy path (US6) + "server is updating" browser banner
stage: green
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: Riaz Mehmood
command: (conversation — "start working on remaining tasks" + add build notification)
labels: [deploy, ux, US6, health, ngrok]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr (ADR-0017 deployment/update strategy)
  pr:
files:
  - src-tauri/src/api/health.rs
  - src/stores/serverStatus.ts
  - src/components/ServerStatusBanner.tsx
  - src/App.tsx
  - deploy/deploy.sh
  - deploy/check-release.sh
  - deploy/noviq-server.service
  - deploy/noviq-deploy.service
  - deploy/noviq-deploy-check.service
  - deploy/noviq-deploy.timer
  - deploy/sudoers.noviq
  - Makefile
  - specs/014-server-thin-client/tasks.md
  - CHANGELOG_DEV.md
tests:
  - cargo check --features server --bin noviq-server (exit 0, 2 pre-existing warnings)
  - tsc --noEmit (clean)
  - bash -n deploy/deploy.sh && bash -n deploy/check-release.sh (OK)
---

## Prompt

User reviewed the F014 connectivity-check.md results (run on a Windows machine, VPN→SSH to the IBA
GPU VM) and decided: move to VPN-free access so users get just a URL + username + password and log
in (ngrok confirmed as the tunnel). Clarified the poll-timer deploy ("push to GitHub → server
updates itself"). Then: "while it builds there must be a notification on user end that server is
building so user understands what is going on, and now start working on remaining tasks."

## Response snapshot

Implemented the US6 deploy/update path as real files (T029–T034) matching deployment-and-updates.md,
and added a user-facing "server is updating" notification as a coherent vertical slice:

- `/api/health` now reports `status:"deploying"` while a `DEPLOYING` sentinel exists under
  `NOVIQ_DATA_DIR` (else `ok`).
- `deploy.sh` touches the sentinel at build start (old binary advertises the in-progress deploy for
  the whole CUDA build), clears it before `systemctl restart` (so the new binary probes `ok`,
  FR-017) and on any exit via trap (banner never sticks; build-failure leaves old server serving).
- Frontend `stores/serverStatus.ts` polls `/api/health` web-transport-only (`isTauri()` no-op),
  maps deploying / fetch-failure(restart blip) → amber `ServerStatusBanner`, auto-reloads on a new
  `version`. Wired in `App.tsx`.
- systemd units + poll-timer + check-release.sh + sudoers + root Makefile `deploy:` target.

## Outcome

- ✅ Impact: unattended `make deploy` path is code-complete (install pending on VM); testers see a
  friendly updating banner through build + restart instead of a broken page.
- 🧪 Tests: `cargo check --features server` exit 0; `tsc --noEmit` clean; `bash -n` both scripts OK.
  Live rollback test (T035) + end-to-end deploy pending on the VM.
- 📁 Files: 1 server (health.rs), 3 frontend (store/banner/App), 8 deploy files, Makefile, tasks.md,
  CHANGELOG_DEV.md.
- 🔁 Next prompts: T035 rollback test; install units on VM; ngrok service unit; then US1 endpoint
  migration (T010/T012/T013) and US4 concurrency queue (T022–T025).
- 🧠 Reflection: sentinel-before-restart ordering is the subtle correctness point; the banner needs
  BOTH the deploying status (long build) and unreachable-fetch (short restart) to cover the window.

## Evaluation notes (flywheel)

- Failure modes observed: none at compile/type level; runtime banner behavior unverified (no VM).
- Graders run and results (PASS/FAIL): cargo check PASS, tsc PASS, bash -n PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on the VM, run deploy.sh against a deliberately-broken
  build to confirm rollback + that the banner clears.
