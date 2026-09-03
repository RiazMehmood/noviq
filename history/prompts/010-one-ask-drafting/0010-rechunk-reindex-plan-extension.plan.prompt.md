---
id: 0010
title: Re-chunk & re-index plan extension (010)
stage: plan
date: 2026-06-15
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: /sp.plan (re-chunk & re-index extension)
labels: [plan, rechunk, reindex, structure-parser, coalescing, 010-extension]
links:
  spec: specs/010-one-ask-drafting/spec-rechunk-reindex-extension.md
  ticket:
  adr:
  pr:
files:
  - specs/010-one-ask-drafting/plan-rechunk-reindex-extension.md (new — plan)
tests:
  - "n/a (planning artifact); plan defines coalescing/repair/label unit tests + Constitution no-regress + fragmented-rule golden"
---

## Prompt

commit the spec then plan it

(/sp.plan input) Plan the re-chunk & re-index extension. Key decision: revive Feature 007's dormant
deep_reindex/chunks_replace.rs ("one chunk per Section, sub-section merging when short") vs a generic
small-leaf coalescing pass in structure_parser.rs — must not create a third chunking path. Reuse the
existing fresh-re-index-from-source path. Offline/local-first, bundled Postgres+pgvector, re-embed via
fastembed, preserve verbatim contract, no Constitution regression. Extension of 010 on branch 011 — no new branch.

## Response snapshot

Committed spec (`0479568e`) first. setup-plan.sh clobbered 011's plan.md with the blank template →
restored it via git checkout; authored the plan as a 010 extension doc instead (no branch coupling).

KEY DECISION RESOLVED: implement a GENERIC `coalesce_undersized_leaves` in structure_parser.rs,
mirroring the existing `split_oversized_leaves` post-order transform. Rejected reviving 007
chunks_replace — verified it is a **2-line stub** (no design to revive) bound to the unbuilt deep-IR/
Mistral tier. Generic coalescing keeps ONE chunking path (split for big, coalesce for small), runs on
the fast tier (all docs), unit-testable without a model.

Three components → one existing pipeline: (1) coalescing (US1); (2) extend existing `text_normalizer`
with conservative verbatim-safe intra-word/broken-line repair (US2); (3) hierarchical label fix +
year-not-number via decompose helpers (US3). Apply via existing `indexer::do_index` re-index path
(FK-safe wipe, progress, resumability) + a Settings re-index trigger gated CAUTIOUS/DANGEROUS (§V).
No new migration, module, or re-index mechanism. Constitution check PASS (only §IV inherited from 011,
not worsened). Phasing: Slice 1 coalesce+re-index (MVP), Slice 2 repair, Slice 3 labels.

## Outcome

- ✅ Impact: implementable plan with the central architecture decision resolved (generic coalescing, not 007); reuses existing pipeline.
- 🧪 Tests: n/a (plan). Defines unit tests + Constitution no-regress + fragmented-rule golden as gates.
- 📁 Files: plan-rechunk-reindex-extension.md under specs/010.
- 🔁 Next prompts: /sp.tasks to break Slices 1–3 into tasks; confirm coalescing thresholds.
- 🧠 Reflection: checking the code turned the "revive vs generic" question from a judgment call into a fact (007 is a stub) — the generic mirror of split_oversized_leaves is obviously right.

## Evaluation notes (flywheel)

- Failure modes observed: setup-plan.sh is branch-coupled and overwrote 011's plan.md (restored from git) — extension specs need manual plan placement.
- Graders run and results (PASS/FAIL): Constitution gate PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): /sp.tasks Slice 1 (coalescing) + a threshold-tuning task validated on a sample re-index.
