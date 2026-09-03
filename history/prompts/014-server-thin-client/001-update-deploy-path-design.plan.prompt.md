---
id: 001
title: Update / deploy path design for GPU server
stage: plan
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: (freeform) start working on 014 + design update path
labels: [deployment, ci-cd, gpu, server]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/014-server-thin-client/deployment-and-updates.md
  - specs/014-server-thin-client/spec.md
  - CHANGELOG_DEV.md
tests:
  - none (design/spec change; no code yet)
---

## Prompt

recall memory and start working on 014 feature, but how we will update it, as i want
to update it also as it runs on gpu, we also need a path to create update locally then
push to github and sync on our server and frontend as required so design accordingly

## Response snapshot

Recalled 014 + GPU-VM deployment memories; confirmed 014 is still spec-only (no HTTP
server crate exists). Identified the hard constraint: CUDA/cuBLAS must compile on the GPU
box, so the server is the build host and the laptop pushes source only. Asked two forks;
user chose **auto-deploy on git push** + **brief-restart OK**.

Designed the update path: edit locally → push to `release` ref → signed GitHub webhook
(`/__deploy`, HMAC-SHA256) → server triggers a **decoupled** `noviq-deploy.service` (so it
survives the restart it causes) → `deploy.sh` rebuilds frontend (`npm run build`) + backend
(`cargo build --release`, CUDA) → restart → health probe → auto-rollback on failure.
Single binary serves API + static frontend (no version skew). Poll-timer fallback for
VPN-gated boxes. Wrote `deployment-and-updates.md` (full scripts + systemd units + sudoers +
webhook spec + laptop `make deploy`); added US6, FR-013…FR-018, SC-007/008 to spec.md;
logged CHANGELOG_DEV entry.

## Outcome

- ✅ Impact: 014 now has a concrete, repeatable update path; unblocks iterative pilot once the HTTP-API split is built.
- 🧪 Tests: none — design/spec only; acceptance checks listed in the design doc §5.
- 📁 Files: deployment-and-updates.md (new), spec.md (US6 + 6 FRs + 2 SCs), CHANGELOG_DEV.md.
- 🔁 Next prompts: implement FR-001 HTTP-API seam (axum) + the `/__deploy` handler + `deploy/` units; confirm IBA VPN reachability (webhook vs poll).
- 🧠 Reflection: the self-restart and rebuild-thrash pitfalls were the non-obvious bits; encoding them as FRs prevents a naive implementation.

## Evaluation notes (flywheel)

- Failure modes observed: none (no execution).
- Graders run and results (PASS/FAIL): n/a.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): stand up a minimal axum server with `/api/health` + `/__deploy` to validate the webhook→deploy→restart loop end-to-end on the VM.
</content>
