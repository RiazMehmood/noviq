---
id: 005
title: Server thin-client tasks breakdown
stage: tasks
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.tasks
labels: [tasks, deployment, axum, gpu, server]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/014-server-thin-client/tasks.md
  - specs/014-server-thin-client/research.md
tests:
  - none (tasks stage; test tasks T021/T035 embedded for auth + rollback)
---

## Prompt

Generate tasks.md for feature 014-server-thin-client. IMPORTANT: write to
specs/014-server-thin-client/tasks.md (NOT specs/015 — the active branch is 015 but the feature
is 014; the setup scripts mis-key off the branch, so target 014 explicitly and do not clobber any
015 artifact). Base tasks on specs/014-server-thin-client/{spec.md,plan.md,research.md,
data-model.md,contracts/http-api.md,quickstart.md,deployment-and-updates.md}. Two sequenced
streams: Stream A (HTTP-API seam, FR-001..FR-012) then Stream B (deploy/update path,
FR-013..FR-018). Reflect confirmed VM facts: A40-12Q 12GB vGPU, driver 470 → CUDA ≤11.4; build
under /opt not /home; poll-timer trigger (not webhook); additive migration; single ModelSlot queue.

(Also folded the user-supplied VM-spec image into research.md R2 before generating tasks.)

## Response snapshot

Wrote tasks.md directly to specs/014 (did not run the branch-keyed check-prerequisites script —
all design docs were already in context — to avoid the clobber that hit setup-plan.sh). 42 tasks,
9 phases: Setup (T001–04), Foundational (T005–09: additive migration, axum router + ServeDir,
/api/health, headless binary, transport seam), then by user story in priority order — US1 browser
parity (MVP), US2 auth/TLS, US4 concurrency + ModelSlot queue, US3 GPU (CUDA ≤11.4, 12 GB layer
budget, CPU fallback), US6 deploy/update path (poll-timer, decoupled unit, auto-rollback, Makefile),
US5 RBAC hooks, Polish + SC-001..008 acceptance gate. Test tasks only where they guard a contract
(T021 auth, T035 rollback). Dependencies + parallel sets + MVP scope documented.

## Outcome

- ✅ Impact: 014 is fully planned through tasks; ready for /sp.implement (Stream A first) once on the VM.
- 🧪 Tests: none run — tasks stage. Embedded test tasks for auth + rollback.
- 📁 Files: tasks.md (new, 42 tasks), research.md (R2 updated with confirmed A40/CUDA/disk facts).
- 🔁 Next prompts: /sp.adr deployment-update-strategy; then /sp.implement starting T001 (best done on the VM for the CUDA-linked tasks).
- 🧠 Reflection: kept guarding against branch==feature script assumptions; wrote artifacts to 014 by hand throughout the chain.

## Evaluation notes (flywheel)

- Failure modes observed: none this step (proactively avoided the branch-keyed script).
- Graders run and results (PASS/FAIL): n/a (tasks stage).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): start /sp.implement with T001–T009 locally (crate/feature wiring + migration) since they don't need the GPU, then move CUDA-linked tasks to the VM.
</content>
