---
id: 002
title: Formalize update-path amendment via SDD
stage: spec
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.specify
labels: [spec, deployment, sdd, amendment]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/014-server-thin-client/spec.md
  - specs/014-server-thin-client/deployment-and-updates.md
  - specs/014-server-thin-client/checklists/requirements.md
tests:
  - none (spec-stage; quality checklist = PASS, 1 clarification carried)
---

## Prompt

Amend feature 014-server-thin-client to add the update/deployment path. Context: the GPU
server is the build host (CUDA/cuBLAS must compile where the toolkit+driver live; the laptop
pushes source only). Update flow: edit locally → push to a `release` ref → signed GitHub
webhook (/__deploy, HMAC-SHA256) triggers a decoupled noviq-deploy.service that rebuilds
frontend (npm build) + backend (cargo build --release, CUDA) → restart → health probe →
auto-rollback on failure. Single binary serves API + static frontend (no version skew; brief
restart accepted). Server-side poll-timer is the fallback when the box is VPN-gated and not
inbound-reachable. Models (*.gguf) and secrets never committed. This is already drafted as
US6 + FR-013..FR-018 + SC-007/008 in specs/014-server-thin-client/spec.md and a design doc
deployment-and-updates.md — reconcile/formalize these rather than duplicate.

(Invoked via /sp.specify after the user asked whether SDD was being followed and chose
"re-route through skills".)

## Response snapshot

Did NOT run create-new-feature.sh — the update path deploys 014's own server, so it is an
in-scope amendment to 014, not a new numbered feature (scaffolding one would fragment the
work). Applied the rest of the sp.specify flow to the existing amended spec.md: ran the
spec-quality validation and wrote specs/014-server-thin-client/checklists/requirements.md.
Result: PASS, with one genuine clarification carried forward to /sp.clarify — webhook vs
poll trigger depends on IBA VPN inbound reachability (affects FR-014). Spec stays at the
repo's technical altitude; fine-grained deploy mechanics remain in the plan-stage design doc.

## Outcome

- ✅ Impact: 014 update-path amendment is now SDD-formalized (spec + validated requirements checklist); stages no longer conflated.
- 🧪 Tests: none — spec stage. Checklist verdict PASS.
- 📁 Files: spec.md (US6/FR-013…018/SC-007-008, from prior turn), checklists/requirements.md (new), deployment-and-updates.md (design).
- 🔁 Next prompts: /sp.clarify (resolve webhook-vs-poll once IBA IT confirms reachability) → /sp.plan → /sp.tasks; surface ADR for deploy strategy.
- 🧠 Reflection: the skill's new-feature scaffold was the wrong default for an amendment; recognizing scope ownership (deploy belongs to 014) avoided fragmenting the spec.

## Evaluation notes (flywheel)

- Failure modes observed: skill template assumed a greenfield feature; required override to avoid a spurious branch.
- Graders run and results (PASS/FAIL): spec-quality checklist = PASS (1 clarification carried).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): run /sp.clarify scoped to the single VPN reachability decision rather than a full 5-question pass.
</content>
