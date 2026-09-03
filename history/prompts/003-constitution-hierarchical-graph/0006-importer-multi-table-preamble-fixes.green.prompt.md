---
id: 0006
title: Constitution importer multi-table, article-proviso and preamble fixes
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (conversation)
labels: [constitution, importer, bugfix, scraper]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - tools/constitution_import/src/ir.rs
  - tools/constitution_import/src/main.rs
  - tools/constitution_import/constitution_ir.json
  - tools/constitution_import/render_review_md.py
  - CHANGELOG_DEV.md
tests:
  - cargo test --manifest-path tools/constitution_import/Cargo.toml (2/2 pass)
---

## Prompt

what do i need to check? do i need to compare existing pakistani.org website with
created json file? but this json is difficuilt to read what do you think?

[Follow-up, after a readable Markdown render was produced and spot-checked, the
user reported: preamble missing; Article 8(3) sub-clause (b)/(i)/(ii) absent;
Article 10(4) Explanations absent; Article 18 proviso absent. Then: "ok go ahead
for your recommendations" — plus an architecture question about how future
document collection/extraction should work (built-in runtime extractor vs a
separate engineer-run onboarding tool vs an in-app password-gated tool).]

## Response snapshot

Diagnosed against cached HTML + importer source. Three real defects + one false
alarm (Art 10 explanations were present; the ad-hoc Markdown renderer omitted
them). Root causes: (1) `first_child_named(_,"table")` captured only the first
nested table — amendment-inserted clauses/sub-clauses in sibling tables dropped
(~63 articles); (2) `Article` had no proviso/explanation fields and article-level
`<blockquote>`s were never parsed; (3) `preamble` hardcoded `None`. Fixed
test-first: red test on Art 8(3) two-table case, then `child_tables()` helper,
article-level `parse_blockquotes`, `fetch_preamble()`, `enum_row` bracket-id
normalisation, hardened `report()`, and a committed `render_review_md.py`.

## Outcome

- ✅ Impact: re-scrape recovered +216 clauses / +117 sub-clauses; preamble
  captured; Art 1/8(3)/18 verified complete; 0 bracket-polluted ids.
- 🧪 Tests: `cargo test` 2/2 green (multi_table_subclauses_all_captured,
  article_level_proviso_captured); importer cross-check PASS.
- 📁 Files: ir.rs (+2 Article fields), main.rs (child_tables, fetch_preamble,
  enum_row, report hardening, tests), constitution_ir.json regenerated,
  render_review_md.py new, CHANGELOG_DEV.md entry.
- 🔁 Next prompts: user decision on `[ ]` bracket-scar handling; resume S2.
- 🧠 Reflection: the importer's own cross-check passed while silently dropping
  216 clauses — count-only acceptance gates miss structural completeness.

## Evaluation notes (flywheel)

- Failure modes observed: single-table assumption; missing schema fields;
  unreviewed renderer caused a false-positive bug report.
- Graders run and results (PASS/FAIL): cargo test PASS; importer report PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): decide whether the IR keeps `[ ]`
  amendment markers or strips them to operative-text-only.
