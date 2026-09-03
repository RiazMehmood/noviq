---
id: 016
title: VM-handoff prep — GPU layers, auth/rollback tests, runbook
stage: green
date: 2026-06-26
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 014-server-thin-client
user: Riaz Mehmood
command: prepare everything so only small tasks remain on the GPU VM
labels: [F014, US3, US6, handoff, deploy, gpu]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr (ADR-0017)
  pr:
files:
  - src-tauri/src/services/llm.rs — T027 NOVIQ_GPU_LAYERS + CPU fallback
  - src-tauri/tests/api_auth.rs (new) — T021 DB-backed auth flow
  - src-tauri/Cargo.toml — register api_auth test
  - deploy/deploy.sh — env-overridable vars + HEALTH_URL
  - deploy/test_deploy_rollback.sh (new) — T035 rollback test (green locally)
  - specs/014-server-thin-client/quickstart.md — T028 latency recipe, tests, ngrok
  - specs/014-server-thin-client/RESUME_ON_VM.md (new) — ordered VM worklist
tests:
  - cargo check --features server --bin noviq-server (exit 0)
  - cargo check --lib (desktop, exit 0)
  - cargo test --features server --test api_auth --test api_admin --no-run (compiles)
  - bash deploy/test_deploy_rollback.sh (PASS=7 FAIL=0)
---

## Prompt

Moving to a Windows machine to drive the GPU VM. Prepare everything now so only
small tasks remain on the VM; implement a little while the VM runs.

## Response snapshot

Front-loaded all non-GPU work. T027: wired `NOVIQ_GPU_LAYERS` into ModelSlot::load
(override / CPU-fallback / log). T021: wrote `tests/api_auth.rs` (DB-backed HTTP auth
flow), compiles, #[ignore]d for the VM. T035: wrote `deploy/test_deploy_rollback.sh`
that runs the real deploy.sh against a stubbed throwaway repo — made deploy.sh vars
env-overridable to enable it — and it's GREEN locally (7/7), proving rollback + the
release-live path without a GPU. Filled quickstart.md with the latency recipe, test
commands, and ngrok note; wrote RESUME_ON_VM.md as the single ordered VM checklist.

## Outcome

- ✅ Impact: VM session reduced to: cuda build, run two #[ignore]d test suites, record
  latency, deploy, tick the acceptance gate, finalize the runbook.
- 🧪 Tests: server bin + desktop lib compile; auth/admin tests compile; rollback test 7/7.
- 📁 Files: 3 new (api_auth.rs, test_deploy_rollback.sh, RESUME_ON_VM.md) + 4 edited.
- 🔁 Next prompts (on VM): T026 cuda build, T028 latency, T052 acceptance, T051 runbook.
- 🧠 Reflection: the rollback test was the big win — env-overridable deploy.sh + tool
  stubs turned an "only-on-VM" SC-008 check into a green local test.

## Evaluation notes (flywheel)

- Failure modes observed: rollback test first failed because the throwaway repo lacked
  `src-tauri/` (deploy.sh `cd src-tauri` aborted before rollback) — fixed the fixture.
- Graders run and results (PASS/FAIL): all four checks PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on the VM, capture the CPU-vs-GPU numbers and
  run the #[ignore]d suites green against bundled PG.
