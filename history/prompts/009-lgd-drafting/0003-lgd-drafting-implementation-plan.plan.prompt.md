---
id: 0003
title: LGD drafting implementation plan
stage: plan
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: /sp.plan
labels: [plan, lgd-drafting, architecture, reuse]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/009-lgd-drafting/plan.md
  - specs/009-lgd-drafting/research.md
  - specs/009-lgd-drafting/data-model.md
  - specs/009-lgd-drafting/contracts/drafting-commands.md
  - specs/009-lgd-drafting/quickstart.md
  - CLAUDE.md
tests:
  - none (plan stage); acceptance harness planned: src-tauri/tests/lgd_drafting_acceptance.rs
---

## Prompt

/sp.plan (no extra args). Generate the implementation plan + Phase 0 research + Phase 1 design artifacts for Feature 009 (LGD Summary & Note Drafting) from the clarified spec, checking against the 13-principle constitution.

## Response snapshot

Ran setup-plan.sh; loaded spec + constitution; surveyed existing seams (intent_router/decompose.rs Fix B helpers, query_router.rs, indexer.rs, file_watcher.rs, exporters/, heading_detector/, db/schema.rs). Produced:

- **plan.md** — Technical Context (Rust 1.87/Tauri + SolidJS, echo.db, additive migration), Constitution Check (all 13 PASS; XI Citation Discipline is the core; III honored by parallelizing retrieval only, never LLM calls), and a single-Tauri-app structure with a new self-contained `services/drafting/` module (ingest, template store, docset, gaps, render) + `commands/drafting.rs` + frontend `components/drafting/`. Framed as a composition feature.
- **research.md** — resolved the 3 deferred items: R1 cited-slot retrieval mechanics (scoped query → `decompose::resolve_provision_label` title-match selector → verbatim + deterministic citation, else refuse-or-flag); R3 latency (draft = progress-reported batch; parallel retrieval, sequential prose < 10s each); R4 template lifecycle (additive SQLite tables, overwrite-on-reconfirm, no v1 versioning). Plus R2 rules/case provenance split, R5 ingestion via reused extraction + heading_detector, R6 watcher reuse, R7 output shape.
- **data-model.md** — persisted `template_schema`/`template_slot`/`gold_example` (+ SC-007 no-retrieval invariant) and in-memory DraftRequest/DocRef/Gap/GapResolution/DraftOutput block types; FR mapping.
- **contracts/drafting-commands.md** — Tauri commands (ingest_template, confirm_template, list/get/delete, start_draft, update_doc_set, resolve_gap, render_draft, export_draft) + `drafting://progress` & `drafting://docset-updated` events + contract acceptance checks.
- **quickstart.md** — end-to-end walkthrough + SC smoke checks.
- Ran update-agent-context.sh (CLAUDE.md Active Technologies updated for 009).

Post-design Constitution re-check: all PASS, no Complexity Tracking entries.

## Outcome

- ✅ Impact: Implementation-ready plan; feature scoped as a composition over existing services with one new module + one additive migration.
- 🧪 Tests: none yet; acceptance harness `lgd_drafting_acceptance.rs` planned with deterministic citation paths (no-LLM where possible).
- 📁 Files: plan.md, research.md, data-model.md, contracts/drafting-commands.md, quickstart.md, CLAUDE.md
- 🔁 Next prompts: /sp.tasks (then /sp.implement). Consider /sp.adr for the deterministic-citation-in-drafting + slot-schema decision (cross-cutting).
- 🧠 Reflection: Reusing decompose's Fix B selector keeps the citation-grade contract in exactly one place across Q&A (008) and drafting (009).

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): Constitution Check — PASS (13/13), pre- and post-design.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in /sp.tasks, sequence US1 (render from confirmed set, reusing decompose helpers) first as the MVP slice, gated behind a tiny synthetic template+corpus fixture until real LGD gold arrives.
