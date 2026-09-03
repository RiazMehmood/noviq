---
id: 003
title: Clarify deploy trigger for VPN-gated VM
stage: spec
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: /sp.clarify
labels: [clarify, deployment, vpn, networking]
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
  - none (clarify stage)
---

## Prompt

i connected to vpn and ssh into that also, but via my windows os, there is issue in ubuntu
so go for it is clear now.

## Response snapshot

The single open clarification (FR-014 trigger: webhook vs poll) is resolved by *how* the user
connected: the VM is reachable only through the Easy Connect VPN at a private address
(~10.150.x.100). A private VPN address has no inbound path from GitHub, so a push webhook cannot
reach it. Decision: **server-side poll-timer is the PRIMARY deploy trigger** (polls
`origin/release`, no inbound port); the signed `/__deploy` webhook is demoted to an optional
future upgrade if IBA later exposes a public HTTPS endpoint. Encoded in spec.md (new
Clarifications section + FR-014 rewrite), design doc §2/§3.4/§3.5/§6, and the requirements
checklist open-item closed.

## Outcome

- ✅ Impact: FR-014 deploy mechanism is now decided and matches the real (VPN-only) network; no open clarifications remain for 014's update path.
- 🧪 Tests: none — clarify stage.
- 📁 Files: spec.md (Clarifications + FR-014), deployment-and-updates.md (poll primary, timer unit added), checklists/requirements.md (item 1 resolved).
- 🔁 Next prompts: /sp.plan (formal plan from the design doc + FR-001 HTTP-API server), then /sp.tasks; ADR for deploy strategy.
- 🧠 Reflection: the connectivity method itself answered the spec question — private VPN IP rules out inbound webhooks, so poll was the only robust primary.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): checklist still PASS, now with zero open clarifications.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): in /sp.plan, validate poll-timer cadence (2 min) against acceptable deploy latency for the pilot.
</content>
