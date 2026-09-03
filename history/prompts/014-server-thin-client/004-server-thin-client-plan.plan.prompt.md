---
id: 004
title: Server thin-client implementation plan
stage: plan
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.plan
labels: [plan, deployment, axum, gpu, server]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/014-server-thin-client/plan.md
  - specs/014-server-thin-client/research.md
  - specs/014-server-thin-client/data-model.md
  - specs/014-server-thin-client/contracts/http-api.md
  - specs/014-server-thin-client/quickstart.md
tests:
  - none (plan stage)
---

## Prompt

Plan feature 014-server-thin-client. Treat specs/014-server-thin-client/deployment-and-updates.md
as the seed design for the update/deploy path (poll-timer primary, decoupled deploy unit,
auto-rollback). The plan must cover BOTH the prerequisite FR-001 HTTP-API server seam (axum
headless server replacing Tauri IPC, single binary also serving the static SolidJS frontend) AND
the deploy/update path (FR-013..FR-018). Note current state: no HTTP server crate exists yet
(src-tauri is the Tauri monolith). Constraints: server is the CUDA build host; VM is VPN-gated
private IP (poll-timer, not webhook); migrations stay additive; models/secrets never in git.

## Response snapshot

`setup-plan.sh` keyed off the active branch (015) and overwrote the committed `specs/015/plan.md`
with a blank template — restored it via `git checkout` (no harm) and wrote the 014 plan directly
to the correct feature dir instead. Authored plan.md (two sequenced streams: A = HTTP-API seam +
headless axum binary serving static frontend + auth/session + single-ModelSlot queue + CUDA
feature; B = poll-timer deploy + decoupled unit + auto-rollback + Makefile), plus research.md
(8 decisions incl. poll-vs-webhook, unverified GPU→CPU fallback, DB-backed argon2id sessions,
Caddy TLS, ServeDir SPA, transport.ts seam), data-model.md (additive users/session + reserved
RBAC cols + audit cols), contracts/http-api.md (endpoint-per-command + auth + /api/health +
optional /__deploy), quickstart.md (VM stand-up + update loop runbook). Constitution check: PASS
with two tracked items (Principle I server tension justified; Principle IV PG divergence
pre-existing). Skipped update-agent-context.sh (branch/feature mismatch would mis-attribute tech).

## Outcome

- ✅ Impact: 014 has a complete, gated plan + Phase 0/1 artifacts; ready for /sp.tasks.
- 🧪 Tests: none — plan stage.
- 📁 Files: plan.md, research.md, data-model.md, contracts/http-api.md, quickstart.md (all new under specs/014).
- 🔁 Next prompts: /sp.tasks (dependency-ordered, Stream A then B); /sp.adr deployment-update-strategy.
- 🧠 Reflection: the SpecKit scripts assume branch==feature; on a mismatched branch they target/clobber the wrong dir — must guard every script invocation in this chain.

## Evaluation notes (flywheel)

- Failure modes observed: setup-plan.sh clobbered an unrelated committed plan.md (branch-keyed path); caught and reverted.
- Graders run and results (PASS/FAIL): constitution check PASS (2 justified items in Complexity Tracking).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): in /sp.tasks, likewise write to specs/014 explicitly rather than trusting branch-derived paths.
</content>
