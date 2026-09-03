---
id: 0022
title: ADR model assisted ingestion gate
stage: plan
date: 2026-06-10
surface: agent
model: claude-fable-5
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: riaz
command: /sp.adr model-assisted-template-ingestion
labels: [adr, template-ingestion, option-c, mistral-7b, verbatim-verify-gate]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: null
  adr: history/adr/0012-model-assisted-template-ingestion-with-deterministic-verbatim-verify-gate.md
  pr: null
files:
  - history/adr/0012-model-assisted-template-ingestion-with-deterministic-verbatim-verify-gate.md
tests:
  - none (documentation artifact)
---

## Prompt

run /sp.adr model-assisted-template-ingestion

## Response snapshot

Created **ADR-0012: Model-Assisted Template Ingestion with Deterministic Verbatim-Verify Gate** (Accepted, 2026-06-10).

Decision cluster: Mistral-7B-Instruct-v0.3 Q4_K_M (reused from F006 `models/ir_builder/`) demand-loaded Tier-4 for one-time offline segmentation; free-text JSON + tolerant parse (llguidance grammar = hard C++ abort, dead on this stack); deterministic verbatim-verify gate as the authority (text+provision verbatim substrings, hyphen-tolerant `resolve_provision_label`, category-from-matched-provision, boilerplate never cited); flag-for-review on gate rejection (FR-010a); refuse-not-degrade when model missing/unparseable (FR-010b, heuristic fallback removed); per-draft hot path stays model-free; migration 024 `provision_ref`.

Significance test: PASS all three (impact: model in the ingestion path + failure policy + data model; alternatives: 5 considered, 4 empirically eliminated in the spike; scope: cross-cutting — ingestion, citation contract, slot persistence, hardware tiers).

Existing-ADR scan: ADR-0008 referenced (the contract the gate preserves); **ADR-0010 flagged** — its llguidance-grammar premise is contradicted by the crash evidence; documented as a divergence in ADR-0012's Negative consequences (0010 not superseded — different pipeline — but its grammar premise is marked questionable if F006 IR is revisited). ADR-0001 referenced (slot discipline).

Grader checklist: clustered decision ✔ · alternatives with rationale ✔ · pros and cons both sides ✔ · concise but complete ✔.

## Outcome

- ✅ Impact: The Option-C decision is now durably documented with full empirical evidence trail (spike outputs on Desktop) and constitution mapping.
- 🧪 Tests: n/a (documentation).
- 📁 Files: history/adr/0012-…md (new).
- 🔁 Next prompts: /sp.tasks to generate Phase D/E tasks from the amended plan.
- 🧠 Reflection: ADR-0010's grammar claim deserved an explicit divergence note rather than silence — same crash class, different pipeline; recorded where future readers of 0010 will find it via the cross-reference.

## Evaluation notes (flywheel)

- Failure modes observed: create-adr.sh requires --title flag (first positional-arg attempt failed).
- Graders run and results (PASS/FAIL): significance checklist PASS; ADR quality checklist PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): /sp.tasks Phase D/E.
