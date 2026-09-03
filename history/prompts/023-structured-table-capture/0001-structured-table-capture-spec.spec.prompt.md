---
id: 0001
title: structured-table-capture spec
stage: spec
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: 023-structured-table-capture
branch: 023-structured-table-capture
user: Riaz Mehmood
command: /sp.specify
labels: [ingestion, tables, chunking, graph, summarization]
links:
  spec: specs/023-structured-table-capture/spec.md
  ticket:
  adr:
  pr:
files:
  - specs/023-structured-table-capture/spec.md
  - specs/023-structured-table-capture/checklists/requirements.md
tests: []
---

## Prompt

General structured-table capture at ingestion. Problem: PDF tables (e.g. RoB Schedule-I 39-department
distribution-of-business table) are extracted by the pdfplumber sidecar as structured SidecarTable{header,
rows} but that structure is only used to linearise the table into prose text and then discarded; the
linearised text is fed through the legal clause parser which shreds it into hundreds of micro-chunks, and
the downstream table_row block_class tag is noisy (over-tags ordinary enumerated rule clauses). Result: no
single place holds the whole table, so 'how many departments' returns 12-14 instead of 39, 'list all
departments' is incomplete, and whole-document summaries drop table content. Goal: a GENERAL solution (any
table in any document type — legal rulebooks, meeting minutes, audit paras, official docs), not RoB-specific.
Thread the authoritative structured SidecarTable data from extraction through to chunking and emit, per
detected table: (a) one whole-table chunk (header + all rows, compact markdown) with unit_type='table' for
counting/list/summary, AND (b) per-row chunks (header + one row) for targeted retrieval — 'both' granularity
confirmed by architect. These table chunks bypass the legal clause parser (no shredding). Also emit a graph
table-node with row children so the planned graph-guided summarizer can walk the table and enumerate all
rows. Deterministic, no LLM. Reuses existing table_sidecar (pdfplumber) extraction. First validation case:
RoB Schedule-I -> counting=39, list=39, summary covers all 39 departments. Additive migration if needed.
Constraints: must not regress existing prose chunking; falls back gracefully when sidecar unavailable; must
scale to thousands of docs (no per-document hand-tuning).

## Response snapshot

Created branch `023-structured-table-capture` and `spec.md`. Grounded the spec in live findings from the
just-completed re-index: RoB Schedule-I is captured as 292 clause micro-chunks (no whole-table unit), and
the existing `block_class='table_row'` tag is NOISY (over-tags ordinary rule clauses, scattered across
chunk_index 4–542) — so the authoritative signal is pdfplumber's structured `SidecarTable{header, rows}`,
currently used only to linearise-then-discard.

Spec: 3 prioritized user stories (P1 count/list completeness = the 39-dept case; P1 summary covers tables;
P2 targeted row lookup), 7 edge cases (no tables, extractor unavailable, ragged/large/multi-page tables,
non-table columnar prose = the over-tag trap, re-index idempotency), 12 FRs (preserve structure through to
unit creation; one whole-table unit holding all rows; per-row units with header; bypass clause parser;
graph table-node with row children; general/deterministic/no-LLM; graceful fallback; no false-positive
tables; row amendment notes; idempotent re-index), 6 measurable SCs (RoB=39; one whole-table unit per
detected table = 100% rows; ≤ rows+1 units per table; zero regression on prose-only docs; summary
represents table; within existing time budget/no model). Assumptions + Out-of-Scope pin boundaries (no new
detector, no OCR table reconstruction, summarizer is a separate feature). Quality checklist: all pass, no
[NEEDS CLARIFICATION] (both-granularity + general scope were architect-fixed pre-spec).

## Outcome

- ✅ Impact: Spec ready for `/sp.plan`; pins the table-chunk contract + graph shape for all future doc types.
- 🧪 Tests: n/a (spec stage)
- 📁 Files: spec.md + checklists/requirements.md on branch 023-structured-table-capture
- 🔁 Next prompts: /sp.plan (thread SidecarTable → table units + graph nodes, additive migration)
- 🧠 Reflection: Grounding the spec in live DB findings (noisy table_row tag) redirected the design from
  "group table_row chunks" to "consume authoritative SidecarTable" — avoided speccing on a bad signal.

## Evaluation notes (flywheel)

- Failure modes observed: existing table_row tagging over-tags non-tables; linearise-then-discard loses structure.
- Graders run and results (PASS/FAIL): spec quality checklist PASS (all items)
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): plan the SidecarTable threading + additive migration for the table unit_type/node.
