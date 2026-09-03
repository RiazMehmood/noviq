# Specification Quality Checklist: noviq.pk Domain Cutover

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-20
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- Items marked incomplete require spec updates before `/sp.clarify` or `/sp.plan`.

### Validation history

**Iteration 1 — 3 items failed, since fixed:**

1. *No implementation details* — FAILED. The first draft named `cloudflared`, systemd,
   Cloudflare, Zoho, PKNIC, SPF/DKIM/DMARC and `~/.cloudflared` throughout the
   requirements. Rewritten to describe the outcome ("the application is reachable at a
   permanent address", "mail passes sender authentication") with vendor names confined
   to Assumptions and Dependencies, where the decisions genuinely belong and where a
   stakeholder reading only the requirements is not blocked by them.
2. *Success criteria technology-agnostic* — FAILED. Drafted criteria included "tunnel
   reconnects within 5 s" and "`nslookup` returns Cloudflare NS", both of which are
   internals. Replaced with user-observable outcomes: a tester reaches the sign-in page
   on the first attempt (SC-001), zero address changes across restarts (SC-002), zero
   messages classified as spam (SC-004).
3. *Requirements testable and unambiguous* — FAILED on the original FR covering
   verification, which read "verify the deployment succeeded". Sharpened into FR-012,
   which names the observation that counts (a page loaded and used, a message
   delivered) and explicitly rules out inferring success from clean logs.

**Iteration 2 — all items pass.** No `[NEEDS CLARIFICATION]` markers were needed: the
two decisions that could have blocked the spec — apex versus subdomain hostname, and
managed versus self-hosted mail — were already settled, the first by the architect
during specification and the second by prior measurement recorded in Assumptions.

### Open risk carried into planning

The spec is complete, but the feature cannot start. Every P1 and P2 dependency is
architect-owned and external (registry panel, DNS/tunnel account, mail account), and
the registry currently holds an empty delegation. Planning should assume a
staged-and-waiting posture rather than a continuous implementation run, and should
make the delegation check an explicit gate rather than an assumption — the previous
six days were spent waiting on propagation for nameservers that had never been set.
