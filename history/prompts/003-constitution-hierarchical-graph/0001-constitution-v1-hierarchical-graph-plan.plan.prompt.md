---
id: 0001
title: constitution v1 hierarchical graph plan
stage: plan
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: /sp.plan
labels: [legal-rag, hierarchical-retrieval, graph, constitution, sqlite, planning]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: N/A
  adr: pending — /sp.adr legal-corpus-hierarchical-graph-model
  pr: N/A
files:
  - specs/003-constitution-hierarchical-graph/spec.md
  - specs/003-constitution-hierarchical-graph/plan.md
  - specs/003-constitution-hierarchical-graph/research.md
  - specs/003-constitution-hierarchical-graph/data-model.md
  - specs/003-constitution-hierarchical-graph/quickstart.md
  - docs/research/legal-corpus-hierarchy-architecture.md
tests:
  - constitution_v1_acceptance.rs (planned, not yet written)
  - db_inspection scripts 09 citation-accuracy / 10 graph-integrity (planned)
---

## Prompt

`/sp.plan generate the plan` — generate the implementation plan for Constitution
v1 (hierarchical + graph retrieval) from `docs/research/legal-corpus-hierarchy-
architecture.md` §7: 7 stages S1–S7, sub-clause leaves, builds on the existing
Tier 3 branch. Preceded in the same session by decisions: clean source first,
graph + hierarchical in one version, sub-clause leaves indexed; and the user's
question on graph-store feasibility at production scale (1000s of documents).

## Response snapshot

Ran the `/sp.plan` Constitution Check gate first — it caught a hard conflict:
the earlier "dedicated graph store (CozoDB)" decision violates Constitution
Principle IV ("single `echo.db`; MUST NOT create separate database files").
Halted plan generation and surfaced the conflict. User asked for a
recommendation given production scale.

Recommended **graph as SQLite tables in `echo.db`** (`graph_nodes`,
`graph_edges` + recursive CTEs): retrieval-time graph use is *local
neighbourhood expansion* (1–2 hops from ~20 seeds), so cost is independent of
total corpus size — feasible to ~1M nodes / ~3M edges with composite edge
indexes. CozoDB only pays off for whole-graph analytics, which legal RAG
retrieval does not perform. Constitution Check then passes.

Generated the SpecKit feature `003-constitution-hierarchical-graph`: spec.md,
plan.md (7 stages S1–S7, Constitution Check table, no `contracts/` — internal
feature), research.md (D1–D5), data-model.md (node/edge/leaf model +
migration 014), quickstart.md (A1–A4 acceptance gate). Updated the design doc
§6/§7.1 to the SQLite-tables decision.

## Outcome

- ✅ Impact: v1 implementation plan ready; graph-storage architecture decided
  constitution-compliantly; v1 scoped behind a 4-check acceptance gate (A1–A4)
  that must pass before v2/OCR.
- 🧪 Tests: none run (planning stage); acceptance tests A1–A4 defined for S7.
- 📁 Files: 5 SpecKit artifacts under specs/003-…; design doc updated.
- 🔁 Next prompts: `/sp.tasks` to break S1–S7 into dependency-ordered tasks;
  `/sp.adr legal-corpus-hierarchical-graph-model` for the architectural record.
- 🧠 Reflection: the Constitution Check gate did its job — it caught an
  over-engineering decision (CozoDB) that also violated an invariant, before any
  code was written. Worth checking the constitution at the *decision* point, not
  just at the plan gate.

## Evaluation notes (flywheel)

- Failure modes observed: agent-context update script failed (derived a branch
  path `specs/feature/tier3-hierarchical-retrieval/` that does not exist) —
  non-blocking; SpecKit scripts assume `NNN-` branch names.
- Graders run and results (PASS/FAIL): Constitution Check — PASS (post-decision).
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): run `/sp.tasks`, then begin S1
  (dev-time source importer).
