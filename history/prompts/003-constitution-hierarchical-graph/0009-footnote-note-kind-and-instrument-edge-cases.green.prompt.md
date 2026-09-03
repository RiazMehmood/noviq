---
id: 0009
title: Footnote note kind and instrument edge-case fixes
stage: green
date: 2026-05-21
surface: agent
model: claude-opus-4-7
feature: 003-constitution-hierarchical-graph
branch: feature/tier3-hierarchical-retrieval
user: Riaz Mehmood
command: (conversation)
labels: [constitution, importer, amendments, bugfix]
links:
  spec: specs/003-constitution-hierarchical-graph/spec.md
  ticket: null
  adr: history/adr/0007-legal-corpus-hierarchical-and-graph-document-model.md
  pr: null
files:
  - tools/constitution_import/src/main.rs
  - tools/constitution_import/constitution_ir.json
  - CHANGELOG_DEV.md
tests:
  - cargo test --manifest-path tools/constitution_import/Cargo.toml (7/7 pass)
---

## Prompt

[S1 spot-check continued. The user pasted the full footnote glossary from
constitution_ir.md and noted ~15 entries still showed an empty instrument
("174 (other) — —").]

## Response snapshot

Investigated the 15 by their `raw` text. Findings: 14 are editorial reference
footnotes ("For the Rules …, see Gazette of Pakistan …") — not amendments, so
no instrument exists; 1 (197) is a pakistani.org source defect ("Substituted
by , Article 3(1)" — instrument blank in the source). Two further issues: 939
("omitted by (with effect from …)" — `extract_instrument` returned garbage
because the `(with effect` marker required a leading space) and 371 (section
written as a bare ", 52" that leaked into the instrument). Fixed: added a `note`
change-kind for editorial footnotes (`raw` starts with "For "); dropped the
leading space on the `(with effect` marker; strip a trailing bare `, <≤3 digits>`
section ref. Test-first; 7/7 green.

## Outcome

- ✅ Impact: footnote kinds now substituted 518 / inserted 301 / omitted 100 /
  note 14 / other 17; only 3 amendment-type footnotes lack an instrument
  (197/823/939 — all source-text defects); 371 and 939 corrected.
- 🧪 Tests: `cargo test` 7/7 (added `editorial_footnote_classified_as_note`,
  `bare_section_ref_trimmed_from_instrument`).
- 📁 Files: main.rs (`classify_change` +note, `extract_instrument` marker +
  trailing-section-ref strip), constitution_ir.json regenerated,
  CHANGELOG_DEV.md caveats corrected.
- 🔁 Next prompts: user re-spot-check; close S1; start S2.
- 🧠 Reflection: "missing data" is sometimes correct data — not every footnote
  is an amendment; a `note` kind makes that explicit instead of an empty field.

## Evaluation notes (flywheel)

- Failure modes observed: editorial footnotes mislabelled `other` with an empty
  instrument read as a bug; marker required a leading space it did not always
  have.
- Graders run and results (PASS/FAIL): cargo test PASS; importer report PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): close S1; begin S2 structure parser.
