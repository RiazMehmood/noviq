---
id: 0003
title: adr composition and db portability
stage: plan
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: /sp.adr
labels: [adr, architecture, composition, postgresql, portability]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0013-one-ask-summary-composition-architecture.md, history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  pr: null
files:
  - history/adr/0013-one-ask-summary-composition-architecture.md
  - history/adr/0014-database-portability-seam-for-postgresql-re-platform.md
  - specs/010-one-ask-drafting/plan.md (Portability constraint added)
tests:
  - n/a (documentation stage)
---

## Prompt

/sp.adr one-ask-composition-architecture and one more thing we are also planning to change db from sqlite to postgresQl also. so design accordingly.

## Response snapshot

Extracted 2 decision clusters from 010 planning artifacts + user directive; both passed the 3-part significance test (impact/alternatives/scope). **ADR-0013 (Accepted): One-Ask Summary Composition Architecture** — built-in 5-section skeleton replaces per-case slot templates; regex intent parse; per-section vec0 KNN retrieval → verbatim provision cards (labels via decompose); Mistral-7B free-text two-lane marker composition ([CITE:n]/[FIG:n], model owns placement, deterministic code owns content); splice + verbatim gate + stray-number post-scan → flags; per-section revision + staged progress. Alternatives: 009 templates (failed live), mega-prompt, llguidance (crashes), 3B/0.5B (unfaithful), cloud (constitution §I), trust-the-prompt (V4 disproved). Extends ADR-0008; supersedes ADR-0012's officer-facing role. **ADR-0014 (Accepted-seam / Proposed-migration): Database Portability Seam for PostgreSQL Re-Platform** — user's parked client/server LAN pivot (Postgres+pgvector) now binds 010 design: single retrieval.rs vector seam (vec0 MATCH/k inside, pgvector-swappable), repository-shaped state.rs with versioned state_json (→JSONB), ANSI-portable SQL in new code, pool-handle discipline as future transport seam; explicitly NOT introducing Postgres/dual-backend now; constitution §IV amendment required before actual migration (future ADR). Conflict noted: §IV vs pivot — resolved by scoping this ADR to design rules only. plan.md updated with a Portability constraint block so /sp.tasks inherits the rules. Grader checklist: clustered ✓, alternatives with rationale ✓, pros+cons both ✓, concise ✓ — PASS both.

## Outcome

- ✅ Impact: The two architecture-defining decisions of the pivot are now permanent records; 010 task generation will inherit the seam rules.
- 🧪 Tests: n/a (docs); knn_parity + future pgvector parity named as validation gates inside the ADRs.
- 📁 Files: ADR-0013, ADR-0014, plan.md portability block.
- 🔁 Next prompts: /sp.tasks for 010; future /sp.adr + constitution amendment when the Postgres migration is actually executed.
- 🧠 Reflection: User's "design accordingly" was the key scope call — captured as binding design rules (cheap now) instead of premature dual-backend abstraction (expensive now, wrong later).

## Evaluation notes (flywheel)

- Failure modes observed: avoided over-granular ADRs by clustering (skeleton+markers+gate+KNN = one ADR; portability rules = one ADR, not per-rule).
- Graders run and results (PASS/FAIL): significance test 3/3 both ADRs — PASS; checklist grader (cluster/alternatives/pros-cons/concise) — PASS both.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): when the pivot unparks, draft the constitution §IV amendment ADR first and validate pgvector recall parity on the pilot corpus before any data migration.
