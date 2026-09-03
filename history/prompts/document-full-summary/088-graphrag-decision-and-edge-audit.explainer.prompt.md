---
id: 088
title: GraphRAG decision (not Neo4j) and live edge-coverage audit
stage: explainer
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: document-full-summary
branch: feat/document-full-summary
user: Riaz Mehmood
command: (conversation)
labels: [graphrag, retrieval, postgres, architecture, audit]
links:
  spec:
  ticket:
  adr: history/adr/0020-graph-augmented-retrieval-on-postgres-not-neo4j.md
  pr:
files:
  - src-tauri/src/services/query_router.rs (read)
  - src-tauri/src/db/repositories/graph.rs (read)
  - src-tauri/src/services/graph_builder.rs (read)
tests: []
---

## Prompt

User argued a knowledge graph would help under a small context window, then sharpened it: pure vector search does cosine similarity on the query only and misses a provision's amendments / connected rules / other acts; a graph reaches X → connected Y → Z and shows the complete picture. Asked to discuss, then: "audit the edge coverage in the DB and start working on their graph augmented retrieval, and what do u suggest simplified this also. currently we have vec search + fts search -> hybrid union so where the graph augmentation work clear that also." Follow-up: adopt our own GraphRAG, no Neo4j — final.

## Response snapshot

Conceded the correctness/completeness point: similarity retrieves what's *similar to the query*, not what's *legally connected to the answer* (amendment / non-obstante / cross-ref / parent Act are lexically dissimilar → missed). Reframed: this is a retrieval-wiring upgrade on the Postgres graph we already run, NOT a reason for Neo4j. Located the pipeline seam (`vec + fts → hybrid union → [summary arm] → salience/entity → build_context`) and the existing bounded recursive-CTE `graph::expand` trapped behind the disabled citation fast-path.

Live edge audit (port 34213, db echo) — the make-or-break finding: `contains` 6,649 (all 13 docs) but `cross_reference` 99 + `non_obstante` 17 **all constitution.pdf**, `amended_by` **0**, every overlay edge within-document. Root cause in `graph_builder.rs`: reference regex matched "Article N" only; `build_graph` runs per-doc with a per-doc citation map → cross-doc impossible by construction. Designed seed-and-expand as an additive flag-gated arm; agreed build order 1→0→2→3 and cross-doc strategy (b)+(a) no fuzzy.

## Outcome

- ✅ Impact: Set the whole feature's direction — own Postgres GraphRAG, not Neo4j; pinned the real blocker (empty relational overlay) with live data rather than assumption.
- 🧪 Tests: n/a (read-only audit)
- 📁 Files: read-only across query_router / graph repo / graph_builder
- 🔁 Next prompts: Phase 1 generalized extraction; Phase 0 arm
- 🧠 Reflection: Auditing the live edges before writing traversal code prevented building machinery over data that wasn't there.

## Evaluation notes (flywheel)

- Failure modes observed: overlay existed for one document only; extractor vocabulary was corpus-specific.
- Graders run and results (PASS/FAIL): audit queries PASS (numbers reproduced twice)
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): generalize the extractor and re-measure edge yield on real chunk text.
