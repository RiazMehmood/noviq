---
id: 006
title: ADR deployment and update strategy
stage: misc
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.adr
labels: [adr, deployment, architecture, gpu]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr/0017-server-deployment-and-update-strategy.md
  pr:
files:
  - history/adr/0017-server-deployment-and-update-strategy.md
tests:
  - none (ADR)
---

## Prompt

run the ADR

(deployment-update-strategy for 014: server is CUDA build host; poll-timer on release branch
trigger, NOT webhook, because VM is VPN-private; single binary serves API + static frontend;
decoupled deploy unit; auto-rollback on health probe; additive migrations. Alternatives: webhook,
manual SSH, build-artifacts-and-ship, blue-green, separate crate.)

## Response snapshot

Created ADR-0017 (Accepted) clustering build-host + trigger + artifact-shape + restart/rollback +
migration-safety as one integrated deployment decision. Documented 5 rejected alternatives with
rationale (webhook impossible behind VPN, manual-SSH defeats US6, CI-cross-compile-CUDA fragile,
blue-green overkill, separate-crate larger diff) and positive/negative consequences (single-command
deploy + no version skew + VPN-safe + rollback safety vs ~2-min poll lag + per-deploy CUDA rebuild +
brief restart). Linked to ADR-0014/0015/0016 and the 014 spec/plan/research. No conflicts with
existing ADRs. Significance test PASS (cross-cutting, multiple alternatives, will be revisited).

## Outcome

- ✅ Impact: deployment/update architecture is now a permanent reviewable record (ADR-0017).
- 🧪 Tests: none (ADR).
- 📁 Files: history/adr/0017-server-deployment-and-update-strategy.md.
- 🔁 Next prompts: /sp.implement starting T001–T009 (local-safe foundational tasks); CUDA + deploy tasks on the VM.
- 🧠 Reflection: clustering avoided over-granular ADRs (one deploy ADR, not five); webhook kept as a documented future upgrade rather than a rejected dead-end.

## Evaluation notes (flywheel)

- Failure modes observed: create-adr.sh needed `--title` flag (first invocation without it failed); corrected.
- Graders run and results (PASS/FAIL): significance checklist PASS (impact + alternatives + cross-cutting); clustering grader PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): revisit the webhook-vs-poll decision if/when IBA exposes a public HTTPS endpoint (would flip R1).
</content>
