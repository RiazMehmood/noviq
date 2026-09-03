---
id: 0010
title: footnote_refs rename and renumbered change kind
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (conversation)
labels: [constitution, importer, amendments, refactor]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - tools/constitution_import/src/ir.rs
  - tools/constitution_import/src/main.rs
  - tools/constitution_import/render_review_md.py
  - tools/constitution_import/constitution_ir.json
  - CHANGELOG_DEV.md
tests:
  - cargo test --manifest-path tools/constitution_import/Cargo.toml (7/7 pass)
---

## Prompt

[S1 spot-check continued. The user noted (1) Article 67(1) shows `(amends: 174)`
but footnote 174 is an editorial note ("For the Rules of Procedure … see
Gazette …"), not an amendment — and there are many such note refs; (2) several
footnotes classified `other` (7, 111, 229 …) clearly reference a real
instrument and should be classified properly.]

## Response snapshot

Investigated the 17 `other` footnotes: 15 are "Renumbered by …" — a real
amendment action the classifier lacked; only 2 (an Order notice, a joint-sitting
resolution) are genuinely `other`. Added a `renumbered` change kind. For the
mislabelling: a body footnote anchor can mark an amendment *or* an editorial
note, so the node field `amendment_refs` was renamed `footnote_refs`
(Article/Clause/Sub-clause) — the kind is carried by `footnotes[].change`. The
review renderer now labels amendment refs `amends:` and note/other refs `see:`,
so Article 67(1) reads `(see: 174)`.

## Outcome

- ✅ Impact: change kinds — substituted 518 / inserted 301 / omitted 100 /
  renumbered 15 / note 14 / other 2; Article 67(1) now `(see: 174)` not
  `(amends: 174)`.
- 🧪 Tests: `cargo test` 7/7 (test renamed `footnote_refs_are_per_node`).
- 📁 Files: ir.rs + main.rs (`amendment_refs`→`footnote_refs`,
  `classify_change` +renumbered), render_review_md.py (amends/see split),
  constitution_ir.json regenerated, CHANGELOG_DEV.md.
- 🔁 Next prompts: user re-spot-check; close S1; start S2.
- 🧠 Reflection: a field name encodes an assumption — `amendment_refs` assumed
  every body footnote is an amendment; `footnote_refs` + a typed `footnotes`
  table states the truth and lets S4 build `amended_by` edges selectively.

## Evaluation notes (flywheel)

- Failure modes observed: editorial-note footnotes mislabelled as amendments in
  the body; renumbering actions dumped into `other`.
- Graders run and results (PASS/FAIL): cargo test PASS; importer report PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): close S1; begin S2 structure parser.
