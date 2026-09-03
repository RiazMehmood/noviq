---
id: 0002
title: structured-table-capture plan
stage: plan
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: 023-structured-table-capture
branch: 023-structured-table-capture
user: Riaz Mehmood
command: /sp.plan
labels: [ingestion, tables, chunking, graph, migration, plan]
links:
  spec: specs/023-structured-table-capture/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/023-structured-table-capture/plan.md
  - specs/023-structured-table-capture/research.md
  - specs/023-structured-table-capture/data-model.md
  - specs/023-structured-table-capture/contracts/table-unit-contract.md
  - specs/023-structured-table-capture/quickstart.md
tests: []
---

## Prompt

/sp.plan (no extra args) — generate the implementation plan for feature 023-structured-table-capture from
its spec.

## Response snapshot

Ran setup-plan; loaded spec + constitution. Produced plan.md, research.md, data-model.md,
contracts/table-unit-contract.md, quickstart.md; ran update-agent-context (claude).

**Technical approach (research R1, the key decision):** in-band region markers (`⟦TBL:{id}⟧…⟦/TBL:{id}⟧`)
in the reflowed text PLUS a structured `ExtractedTable{id,page,header,rows}` list carried on the parse result.
Marked regions are atomic (not clause-parsed); units come from the structured `ExtractedTable`, not the noisy
`block_class='table_row'` layout tag (rejected) nor fragile post-hoc region matching (rejected). Reuses the
existing text flow → smallest viable diff.

**Data model:** two new `chunks.unit_type` values (`table` whole-table markdown = header+all rows;
`table_row` header-labelled cells) and two `graph_nodes.node_type` values (`table`, `table_row`) with the
table node `contains` its row nodes (reuses existing tree edge → `graph::expand` walks it for free). Additive
**migration 013** widens the two CHECK vocabularies (superset; no drop). Idempotent via `do_index`
delete-then-insert.

**Contract (C-1..C-6):** row count == N; exactly 1+N units per table; table node has N row children; markers
never in stored content; prose-only docs byte-identical (no regression); no table units where no table
detected.

**Constitution gate PASS** — one pre-existing divergence logged in Complexity Tracking (§IV SQLite → live
store is bundled PostgreSQL per ADR-0015; feature adds only an additive migration, introduces no new store).
Advances XIII (graph-as-first-class: table node). Source changes localised to parsers/table.rs + pdf.rs,
structure_parser, structured_importer, graph_builder + one migration.

## Outcome

- ✅ Impact: Implementation-ready plan; table-unit + graph-node contract pinned for all doc types.
- 🧪 Tests: n/a (plan stage); test strategy = reflow marker round-trip + contract C-1..C-6 + live RoB=39.
- 📁 Files: plan.md, research.md, data-model.md, contracts/, quickstart.md
- 🔁 Next prompts: /sp.tasks
- 🧠 Reflection: The marker+sidecar threading keeps the diff small vs re-plumbing structured tables through
  every stage; keying units off ExtractedTable (not the noisy layout tag) is what makes it correct.

## Evaluation notes (flywheel)

- Failure modes observed: golden-gated reflow text will change with markers → must update goldens + add round-trip test.
- Graders run and results (PASS/FAIL): constitution gate PASS; spec quality checklist PASS (from spec stage)
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): /sp.tasks to break into ordered, testable tasks.
