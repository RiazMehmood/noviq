---
id: 0007
title: Constitution importer amendment metadata pass
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (conversation)
labels: [constitution, importer, amendments, scraper]
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
  - cargo test --manifest-path tools/constitution_import/Cargo.toml (4/4 pass)
---

## Prompt

[S1 spot-check continued. The user reported, from Articles 204 and 207 vs
pakistani.org, that amendment references on top of an article (the `667[ … ]667`
wrapper) were dropped, that the empty `680[]680` deletion marker landed in the
wrong place, and asked how omitted positions should be modelled. Then approved
Option B (per-node refs + record-position-only, no fabricated `(c)` label). The
user then pasted the pakistani.org Amendments catalogue page and asked whether /
how to capture it; chose option (A): fold the catalogue + footnote descriptions
into one coherent amendment pass.]

## Response snapshot

Diagnosed three layers of amendment data and implemented all three.
(1) Body markers → per-node `amendment_refs` on Article/Clause/Sub-clause via
`amendment_refs_own` (does not descend into child tables); article-level
wrappers (667) captured via an `N[`/`]N` toggle in `following_tables`.
(2) Footnote descriptions → `parse_references` parses each page's
`<div class="references">` into `footnotes[]` (change kind, instrument, date,
previous text). (3) Catalogue → `parse_amendments` reads the 24 enacted
Amendment Acts from the index page, skipping the number-colliding "Proposed"
section. `[ ]` brackets stripped from text (amendment data is now structural).
Empty `N[]N` recorded as a normal ref whose footnote `change` is `omitted` — no
fabricated `(c)` label. Test-first; 4/4 green.

## Outcome

- ✅ Impact: Art 204 → `amendment_refs ["667"]` + clause(1) `["668"]`; Art 207(3)
  → `["680"]` with footnote 680 `change: omitted`; 24 amendments, 950 footnotes,
  0 bracket scars.
- 🧪 Tests: `cargo test` 4/4 (multi-table, article-proviso, per-node refs,
  references-block); importer cross-check PASS.
- 📁 Files: ir.rs (+Amendment/Footnote types, +amendment_refs on Clause/
  SubClause), main.rs (amendment_refs_own, wrapper toggle, parse_references,
  parse_amendments, strip_brackets), constitution_ir.json regenerated,
  render_review_md.py, CHANGELOG_DEV.md.
- 🔁 Next prompts: user re-spot-check of the refreshed `constitution_ir.md`;
  then close S1, start S2.
- 🧠 Reflection: the spot-check gate keeps surfacing IR-fidelity gaps the
  count-only acceptance check cannot — amendment data needed three layers, not
  the one flat list the plan assumed.

## Evaluation notes (flywheel)

- Failure modes observed: amendment data under-modelled as a flat number list;
  wrapper anchors outside the article cell; deletion semantics lost.
- Graders run and results (PASS/FAIL): cargo test PASS; importer report PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in S4, resolve each node's
  `amendment_refs` → `footnotes` → `amendments` to build `amended_by` edges.
