---
id: 125
title: Table guard round 2 verified on prod
stage: green
date: 2026-07-29
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [ingestion, tables, corpus-quality, verified]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src-tauri/src/services/parsers/table.rs
- CHANGELOG_DEV.md
tests:
- lib suite 801 passed / 0 failed
- dry-run vs live extraction: Constitution 1/4 -> 4/4 rejected; Delegation 75/75 kept; Manual 63/63 kept
- prod verification after re-index: 7 articles recovered, table chunks 8 -> 0
---

## Prompt

deploy it and i will reindex again → "reindexing done, check it" →
"there are total 280 articles in the consstitute"

## Response snapshot

Round 1 of the guard recovered only Articles 60/61. Asking the sidecar what it
SEES (`op:extract_tables`) revealed why: pdfplumber puts the whole page of
prose in the table HEADER cell (1,581–2,046 chars) and leaves one data row
holding just the footnote rule (133–552 chars) — so a guard measuring data
rows saw "371 chars, too small". Round 2 counts the header as a unit and
treats a 2+ column header as proof of a real table.

Validated by DRY RUN against live extraction before asking for another
re-index (Constitution 1/4→4/4; Delegation 75/75 kept; Manual 63/63 kept),
then verified on prod: Articles 60, 61, 127, 128, 240, 241, 242 recovered with
correct catchline titles and pages; Constitution table chunks 8 → 0; distinct
Articles 266 → 273.

Architect's "280 articles" reconciles exactly: 272 real + 1 false label
(`Article 343`) = 273 stored, 8 genuinely missing (90, 113, 142, 152, 197,
217, 219, 272). Those 8 ARE in pdftotext output but their text is in ZERO
chunks — Article 142's unique phrase ("exclusive power to make laws") is also
absent from the pdfplumber sidecar output, so the loss is at extraction, not
labelling. Investigation continuing (margin-crop suspected).

## Outcome

- ✅ Impact: 7 constitutional articles restored to the corpus, incl. Art 240 (civil service appointments).
- 🧪 Tests: 801/0; dry-run + prod verification both recorded.
- 📁 Files: table.rs (header-aware guard + 2 new tests), CHANGELOG.
- 🔁 Next: diagnose the 8 lost articles (pdfplumber vs pdftotext comparison running); then FALSE labels.
- 🧠 Reflection: DB chunks show only what SURVIVED — diagnose extraction defects against the extractor's own output.

## Evaluation notes (flywheel)

- Failure modes observed: round-1 thresholds derived from surviving chunks missed where the content actually was; generic probe phrases matched the TOC instead of the body.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): compare pdfplumber vs pdftotext per provision opening.
