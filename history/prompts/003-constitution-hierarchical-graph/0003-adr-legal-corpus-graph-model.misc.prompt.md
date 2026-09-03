---
id: 0003
title: adr legal corpus graph model
stage: misc
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: /sp.adr
labels: [adr, legal-rag, hierarchical-retrieval, graph, architecture]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: N/A
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: N/A
files:
  - history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
tests: []
---

## Prompt

`/sp.adr legal-corpus-hierarchical-graph-model` — document the architectural
decision: adopt a generic profile-driven hierarchical + graph document model
for the legal/government corpus; graph stored as SQLite tables in `echo.db`
(`graph_nodes`/`graph_edges` + recursive CTEs), NOT a dedicated engine like
CozoDB, per Constitution Principle IV.

## Response snapshot

Extracted one architecturally-significant **decision cluster** — the integrated
hierarchical + graph document model — and created **ADR-0007**
(`history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md`),
status Accepted. The cluster bundles five decisions that change together:
hierarchy levels, doc-type profiles, sub-clause leaves, SQLite-table graph
storage, and the citation-fast-path retrieval pipeline. Five alternatives
documented with rejection rationale (CozoDB/dedicated engine, article-level
leaves, fixed/semantic chunking, constitution-specific model, OCR-repair).
References ADR-0002 (schema strategy — extended additively) and Constitution
Principle IV. No conflicts with existing ADRs 0001–0006.

## Outcome

- ✅ Impact: permanent reviewable record of the v1 architecture and the
  SQLite-vs-CozoDB / Principle IV reasoning, before implementation begins.
- 🧪 Tests: N/A (documentation artifact).
- 📁 Files: ADR-0007 created; PHR 0003 recorded.
- 🔁 Next prompts: begin implementation at T001, or `/sp.implement`.
- 🧠 Reflection: graph storage + structural model were clustered into one ADR
  (they change together) rather than split — avoids over-granular ADRs.

## Evaluation notes (flywheel)

- Failure modes observed: none — significance test passed (impact, alternatives,
  cross-cutting scope all true); decision is clustered, not atomic.
- Graders run and results (PASS/FAIL): ADR checklist — PASS (clustered decision,
  ≥1 alternative with rationale, pros + cons, concise).
- Prompt variant (if applicable): N/A.
- Next experiment (smallest change to try): start T001–T005 (skeleton +
  migration 014).
