---
id: 0028
title: ADR review legal corpus model
stage: misc
date: 2026-05-24
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: 003-constitution-hierarchical-graph
user: engr.riaz2010@gmail.com
command: /sp.adr legal-corpus-hierarchical-graph-model
labels: [adr, t034, v1-close, legal-corpus, hierarchical-graph]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: ""
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: ""
files: []
tests: []
---

## Prompt

> yes run /sp.adr legal-corpus-hierarchical-graph-model

## Response snapshot

Ran the sp.adr workflow. Loaded planning artifacts via
`.specify/scripts/bash/check-prerequisites.sh` — confirmed
FEATURE_DIR + plan/research/data-model present.

Scanned `history/adr/` and found **ADR-0007 (Legal Corpus Hierarchical
and Graph Document Model, Accepted 2026-05-21)** already documents
this exact decision cluster. Reviewed the ADR end-to-end and
cross-checked each of its 6 architectural decisions against the v1
as-built state — all shipped intact:

| ADR-0007 decision | v1 as-built |
|---|---|
| Hierarchy: Doc → Part → Chapter → Article → Clause → Sub-clause | SHIPPED unchanged (12/29/313/808/325) |
| Doc-type profiles, constitution first | SHIPPED (CONSTITUTION + salient_units field) |
| Leaf granularity = sub-clause | SHIPPED + Idea 1 added paragraph children at clause level for oversize leaves |
| Graph as SQLite tables, no CozoDB | SHIPPED (graph_nodes=1488, graph_edges=1626, all SQLite) |
| HTML import for constitution, OCR for v2 | SHIPPED for constitution; v2 OCR queued |
| Retrieval: citation fast-path → hybrid → reranker | SHIPPED + S1/S2/S3a/S3b post-RRF boost layers |

The post-RRF boost stack (S1/S2/S3a/S3b) and Idea 1's paragraph
splitting are implementation refinements **below** the architectural
decision line — they don't override the hierarchy + graph + retrieval
pipeline; they tune behaviour within it. No conflict; no superseding
ADR required.

Per the sp.adr skill rules ("If covered by existing ADR → note
reference"), no new ADR file is created. T034 is satisfied by
reaffirming ADR-0007 with the as-built confirmation captured in this
PHR and in `docs/research/legal-corpus-hierarchy-architecture.md §8`.

## Outcome

- ✅ Impact: T034 closed. ADR-0007 stands as the authoritative architectural record for feature 003. v1 → v2 transition unblocked at the architecture-doc level.
- 🧪 Tests: n/a
- 📁 Files: none new — existing ADR-0007 referenced; `docs/research/legal-corpus-hierarchy-architecture.md §8` (written in T032) is the as-built supplement.
- 🔁 Next prompts: v2 work — OCR/extraction for govt corpus (Acts/Rules/Notifications), Pass-2 NER scheduler integration (#13), `index_constitution → index_structured_doc` rename (#4), S4 input-layer feature (#5).
- 🧠 Reflection: Pre-existing ADR matched the shipped design with zero drift. The "discuss before code, ship one thing at a time, document as you go" pattern this feature followed gave the planning ADR the same shape as the actual code — no surprises at v1 close.

## Evaluation notes (flywheel)

- Failure modes observed: none. ADR-0007 is comprehensive and accurate.
- Graders run and results (PASS/FAIL): n/a — review/reaffirmation only.
- Prompt variant (if applicable): n/a
- Next experiment: when v2 starts (Acts ingestion), revisit ADR-0007's "doc-type profiles" decision; if a second profile reveals shared structure the constitution profile didn't anticipate, write a follow-up ADR rather than amending 0007 in place.
