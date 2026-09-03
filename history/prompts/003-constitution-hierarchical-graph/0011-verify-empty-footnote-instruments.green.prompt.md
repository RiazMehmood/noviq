---
id: 0011
title: Verify empty footnote instruments are source-faithful
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (conversation)
labels: [constitution, importer, amendments, verification]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - tools/constitution_import/render_review_md.py
  - CHANGELOG_DEV.md
tests:
  - cargo test --manifest-path tools/constitution_import/Cargo.toml (7/7 pass, unchanged)
---

## Prompt

[S1 spot-check. The user listed the 15 footnotes still showing an empty
instrument in the glossary (174, 197, 213, 214, 403, 407, 421, 523, 710, 796,
806, 820, 823, 916, 939) and asked: are they empty intentionally, or is
something still missing? "check this final part."]

## Response snapshot

Checked the cached HTML directly. Verdict: nothing missing on our side.
13 are intentionally empty — 12 `note` + 1 `other` are editorial reference
footnotes ("For the Rules …, see Gazette …"), not amendments, so there is no
instrument to record. 2 (197 substituted, 939 omitted) are pakistani.org source
defects — confirmed in the raw HTML: footnote 197 reads "Substituted by ,
Article 3(1)" and 939 "omitted by  (with effect from …)" with the instrument
literally blank in the source. Unrecoverable. Improved the glossary renderer so
empty entries are self-explanatory: note/other footnotes show their reference
text, the 2 defects show "(instrument blank in pakistani.org source)".

## Outcome

- ✅ Impact: confirmed all 15 empty instruments are correct (13 by design,
  2 source defects); glossary no longer shows a bare "—".
- 🧪 Tests: `cargo test` 7/7 (unchanged — renderer-only change).
- 📁 Files: render_review_md.py (glossary detail), CHANGELOG_DEV.md caveat
  corrected (2 defects, not 3), constitution_ir.md regenerated.
- 🔁 Next prompts: user re-spot-check; close S1; start S2.
- 🧠 Reflection: a bare "—" reads as a defect; showing why a field is empty
  (reference text, or "blank in source") turns a recurring question into a
  self-evident answer.

## Evaluation notes (flywheel)

- Failure modes observed: an intentionally-empty field, displayed as "—", looks
  like missing data and triggers repeated review questions.
- Graders run and results (PASS/FAIL): verified against cached HTML; PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): close S1 (mark tasks.md T009),
  begin S2 structure-aware parser.
