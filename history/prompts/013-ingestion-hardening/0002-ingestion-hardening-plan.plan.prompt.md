---
id: 0002
title: Ingestion hardening plan
stage: plan
date: 2026-06-19
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: Riaz Mehmood
command: /sp.plan
labels: [ingestion, citation-grade, deterministic, fast-tier]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/013-ingestion-hardening/plan.md
  - specs/013-ingestion-hardening/research.md
  - specs/013-ingestion-hardening/data-model.md
  - specs/013-ingestion-hardening/contracts/ingestion-stages.md
  - specs/013-ingestion-hardening/quickstart.md
  - CLAUDE.md
tests:
  - cargo test --lib
  - cargo test --test ingestion_hardening_acceptance
---

## Prompt

/sp.plan (no additional user input)

Execute the implementation planning workflow for feature 013-ingestion-hardening:
run setup-plan, load spec + constitution, fill Technical Context, run Constitution
Check, produce Phase 0 research.md and Phase 1 data-model.md / contracts/ /
quickstart.md, update agent context, re-check constitution, and report.

## Response snapshot

Generated the full plan artifact set for **Fast-Tier Ingestion Hardening (Citation-Grade)**,
grounded in the actual codebase (verified anchors in `text_normalizer.rs`, `generic.rs`,
`structured_importer.rs`, `structure_parser.rs`, `parsers/chrome.rs`, `graph_builder.rs`, and the
PG migrations) and the user's Desktop plan + judge audit.

Key decisions captured:
- **Two-pass spine** (structure → vocabulary) reusing existing `NodeLevel`/`label_status='unresolved'`.
- **Geometry zoning** (not text-regex) for page-numbers + footnotes via existing `Block.y_band` /
  `pdftotext -bbox` `WordBox`.
- **LOCKED phase order**: P0 (D9+D12) → P1 (D5/D6 zoning) → P2 (D2/D3/D4/D10 tree) → P3 (D1 vocab)
  → P4 (D7/D8 boundaries) → US5 graph capture.
- Schema is largely ready (`page_number`, `block_class`, `label_status`, `raw_marker`,
  `boundary_confidence`, `graph_edges` cross_reference/amended_by, `amendment_events`); at most one
  additive migration (footnote anchor) — prefer `domain_metadata` JSON first.
- Constitution Check PASS with one pre-existing, already-tracked Principle-IV divergence (PostgreSQL,
  inherited from F011/F012).

## Outcome

- ✅ Impact: Complete, code-grounded implementation plan for citation-grade fast-tier ingestion;
  ready for /sp.tasks.
- 🧪 Tests: planned — `cargo test --lib` per phase + new `ingestion_hardening_acceptance` frozen
  fixture (177-chunk law doc + prose + table); SC-001…SC-009 gates.
- 📁 Files: plan.md, research.md, data-model.md, contracts/ingestion-stages.md, quickstart.md;
  CLAUDE.md agent context updated.
- 🔁 Next prompts: /sp.tasks to break P0…US5 into dependency-ordered tasks; optionally /sp.adr for
  the two-pass/geometry-zoning architectural decision.
- 🧠 Reflection: The feature is an "evolve, don't rewrite" hardening — most schema + seams already
  exist, so the plan emphasizes layering fixes at the right stage over new structures.

## Evaluation notes (flywheel)

- Failure modes observed: none in planning; risk areas flagged (P2 over/under-firing of bare-number
  detection; footer-band mis-tuning in P1).
- Graders run and results (PASS/FAIL): Constitution Check PASS; spec quality checklist already PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement P0 (JOIN_TARGETS audit + page_offsets thread),
  re-index, diff vs fixture.
