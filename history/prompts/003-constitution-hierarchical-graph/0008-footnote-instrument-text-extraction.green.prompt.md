---
id: 0008
title: Footnote instrument parsed from text not hyperlink
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
  - cargo test --manifest-path tools/constitution_import/Cargo.toml (5/5 pass)
---

## Prompt

[S1 spot-check continued. The user pasted the pakistani.org footnote
descriptions for footnotes 430–493 — every one names its instrument — and
showed that in our `constitution_ir.md` glossary roughly half were empty
("430 (inserted) — —"), while older-amendment ones were populated.]

## Response snapshot

Root cause: `parse_references` extracted the instrument from the footnote div's
`<a href>` link, but pakistani.org only hyperlinks the instrument for older
amendments (those with a dedicated `Namendment.html` page) — 26th/27th/23rd
amendment footnotes are plain text. Fixed by adding `extract_instrument`, which
parses the instrument from the description text: the span after "… by " up to
the section/article citation (`, Section`, `, Article`, `, Art `, `, Sch`) or
the effective-date clause. Coverage rose 702 → 935/950 (98%). Test-first.

## Outcome

- ✅ Impact: footnote `instrument` coverage 702 → 935/950; all footnotes the
  user listed (430, 436, 442, 443, 447, 453, 493) now resolve.
- 🧪 Tests: `cargo test` 5/5 (added `instrument_parsed_from_unlinked_text`).
- 📁 Files: main.rs (`extract_instrument`, `parse_references` no longer needs
  the link selector), constitution_ir.json regenerated, CHANGELOG_DEV.md caveat
  corrected (15/950 remain unparsed).
- 🔁 Next prompts: user re-spot-check; close S1; start S2.
- 🧠 Reflection: structured fields should be parsed from the authoritative text,
  not from optional presentational markup (a hyperlink) that the source applies
  inconsistently.

## Evaluation notes (flywheel)

- Failure modes observed: relying on a hyperlink that the source supplies for
  only some entries.
- Graders run and results (PASS/FAIL): cargo test PASS; importer report PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): map each `instrument` string to a
  catalogue `amendments[]` entry during S4 graph construction.
