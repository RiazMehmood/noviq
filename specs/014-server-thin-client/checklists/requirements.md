# Specification Quality Checklist: Server / Thin-Client Deployment (incl. update path)

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-06-25
**Feature**: [spec.md](../spec.md) · companion design: [deployment-and-updates.md](../deployment-and-updates.md)
**Scope of this validation**: the US6 + FR-013…FR-018 + SC-007/008 update-path amendment (the
rest of 014 was previously scoped).

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
  - NOTE: per house style this 014 spec is a *technical* spec — it names HTTPS, argon2id, CUDA,
    PostgreSQL, webhook/HMAC throughout (pre-existing). The amendment matches that altitude.
    Fine-grained mechanics (deploy.sh, systemd units, sudoers) are kept in the *plan*-stage
    `deployment-and-updates.md`, not the spec body. ✔ acceptable for this repo.
- [x] Focused on user value and business needs — US6 frames the developer's need (iterate without manual SSH builds)
- [x] Written for non-technical stakeholders — readable; "why P1" rationale present
- [x] All mandatory sections completed — Overview, User Scenarios, Requirements, Success Criteria, Assumptions, Non-Goals

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain — the two design forks (trigger=auto-on-push, downtime=brief-restart) were resolved with the architect
- [x] Requirements are testable and unambiguous — FR-013…018 each map to a check in design doc §5
- [x] Success criteria are measurable — SC-007 (push to release → live, no manual SSH), SC-008 (failed build → previous version still running)
- [ ] Success criteria are technology-agnostic — SC-007/008 are outcome-framed; OK. (Spec overall is intentionally technical — see Content Quality note.)
- [x] All acceptance scenarios are defined — US6 has 4 acceptance bullets
- [x] Edge cases are identified — self-restart-kills-deploy, rebuild-thrash, VPN-unreachable webhook, rollback-on-health-fail
- [x] Scope is clearly bounded — `release`-ref trigger only; models/secrets excluded; zero-downtime out of scope
- [x] Dependencies and assumptions identified — server-as-build-host (CUDA), VPN reachability to confirm with IBA IT

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria — FR-013…018 ↔ design-doc §5 checks
- [x] User scenarios cover primary flows — local edit → release push → auto rebuild/restart → rollback
- [x] Feature meets measurable outcomes defined in Success Criteria — SC-007/008
- [x] No implementation details leak into specification — fine-grained mechanics confined to the design doc

## Open items carried to /sp.plan

1. ~~VPN inbound reachability~~ **RESOLVED 2026-06-25** (clarify): VM is VPN-gated at a private
   address → poll-timer is the primary deploy trigger; webhook is a future-only upgrade. FR-014 +
   Clarifications updated.
2. The HTTP-API server crate (FR-001) does not exist yet — the deploy seam (poll-timer + `deploy/`
   units, optional webhook handler) is built alongside it in /sp.plan → /sp.tasks.

## Notes

- Amendment (not a new feature): the update path deploys 014's own server, so it lives inside 014.
  `create-new-feature.sh` was intentionally NOT run (would fragment into a bogus sibling feature).
- Validation result: **PASS** (1 known clarification carried forward: VPN webhook-vs-poll).
</content>
