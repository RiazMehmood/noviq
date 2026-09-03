---
id: 0001
title: 012 doc-intelligence production plan
stage: plan
date: 2026-06-17
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.plan
labels: [plan, ingestion, drafting, citation-grade, sdd]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/012-doc-intelligence-production/plan.md
  - specs/012-doc-intelligence-production/research.md
  - specs/012-doc-intelligence-production/data-model.md
  - specs/012-doc-intelligence-production/contracts/chrome-strip.md
  - specs/012-doc-intelligence-production/contracts/table-engine.md
  - specs/012-doc-intelligence-production/contracts/heading-detection.md
  - specs/012-doc-intelligence-production/contracts/label-provenance.md
  - specs/012-doc-intelligence-production/contracts/deterministic-drafting.md
  - specs/012-doc-intelligence-production/quickstart.md
  - CLAUDE.md
tests:
  - "cargo test --lib parsers:: heading_detector:: structure_parser:: structured_importer:: drafting::one_ask (G2 golden gate — to run at wave slice-end)"
---

## Prompt

recall memory and start working on latest feature

(Followed by /sp.plan to generate the implementation plan for Feature 012 —
Document-Intelligence Production Overhaul. Memory resume point: spec.md committed,
NEXT = write plan.md + tasks.md, then implement Wave A / US1 chrome-strip.)

## Response snapshot

Generated the full plan artifact set for Feature 012 grounded in the codebase:

- **plan.md** — Summary, Technical Context (Rust 1.87, pdftotext -bbox, NO lopdf,
  Postgres+pgvector), Constitution Check across all 13 principles (PASS with one
  documented §IV divergence — pre-existing Postgres port from F011, in Complexity
  Tracking), Project Structure (touched paths: new `parsers/chrome.rs`, evolve
  `parsers/table.rs`, extend `heading_detector/regex_pass.rs` +
  `structure_parser.rs` + `structured_importer.rs`, new migration `003`).
- **research.md** — six decisions incl. R0 disposition of the carried-over US8
  code (KEEP bbox IO + year-rule; EVOLVE table.rs to projection profiles;
  SUPERSEDE the downstream is_boilerplate flag with upstream chrome class;
  repurpose migration 002's column as the retrieval-side exclusion).
- **data-model.md** — additive chunk columns (block_class, raw_marker, page,
  char_span, extraction/reconstruction method, boundary_confidence, label_status);
  Block + TableRowUnit entities; G2 golden fixture set.
- **contracts/** — five internal module contracts (chrome-strip, table-engine,
  heading-detection, label-provenance, deterministic-drafting) each with pure-core
  signatures, invariants (G1/G2), and acceptance.
- **quickstart.md** — per-wave run recipe (build ops, G2 gate, re-index, live verify).
- Ran update-agent-context.sh (CLAUDE.md updated).

## Outcome

- ✅ Impact: Feature 012 is plan-complete; Phase 2 (/sp.tasks) is unblocked.
  Carried-over US8 code disposition is now an explicit, recorded decision
  (evolve, not delete).
- 🧪 Tests: none run (planning only); G2 golden gate defined as the per-wave gate.
- 📁 Files: 9 spec artifacts + CLAUDE.md.
- 🔁 Next prompts: /sp.tasks to generate tasks.md (Wave A/US1 first), then
  /sp.implement.
- 🧠 Reflection: the single material finding is the §IV (SQLite→Postgres)
  divergence; captured as pre-existing + pending amendment rather than blocking.

## Evaluation notes (flywheel)

- Failure modes observed: none (planning).
- Graders run and results (PASS/FAIL): Constitution Check PASS (1 documented divergence).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): generate tasks.md and confirm Wave A
  task ordering matches the one-re-index sequencing.
