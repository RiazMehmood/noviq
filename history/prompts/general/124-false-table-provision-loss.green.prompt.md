---
id: 124
title: False table detection deleted provisions
stage: green
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [ingestion, tables, corpus-quality, beta]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src-tauri/src/services/parsers/table.rs
- scripts/corpus_gap_check.sql
- src-tauri/tests/ (4 dead suites deleted)
- CHANGELOG_DEV.md
tests:
- 4 new table guard tests; lib suite 799 passed / 0 failed (was 795)
- golden delegation-table tests still pass (genuine tables unaffected)
---

## Prompt

yes fix the table swallowing issue first

(preceded by: "1 and article missing issue is severe, and may be this is also
happening with other documents too, how we can check that? how we can confirm
it?")

## Response snapshot

Built `scripts/corpus_gap_check.sql` — audits any corpus for missing
provisions using the fact that provisions are numbered (self-auditing), plus a
triage section that finds where missing text ended up. Ran it: the missing
Constitution articles were stored under `Table (p.89)` etc. as
`| 127. Subject to the Constitution, …`.

Root cause: `reflow_with_sidecar` REPLACES a page's text with the table
rendering, so a false-positive table deletes that page's provisions before
heading detection ever sees them. Fix = `looks_like_prose_not_table`: reject a
region when every row is ONE populated column AND rows are paragraph-sized
(≥400 total, ≥200 mean). Thresholds measured from the corpus (false: 1 row,
1,720–2,600 chars, single column; genuine: 2+ columns, rows 113–576).
Rejection is the safe direction — the page keeps its text.

Also deleted the 4 dead `doc_ir` test suites (item 1 the architect approved).

## Outcome

- ✅ Impact: ~138 trapped provision rows recoverable (Manual 90, Delegation 42, Constitution 6).
- 🧪 Tests: 799/0; golden delegation tables unaffected.
- 📁 Files: table.rs guard + 4 tests, audit script, 4 test files deleted, CHANGELOG.
- 🔁 Next: RE-INDEX required for existing docs (architect runs it on GPU); then re-run the audit. Still open: 7 Constitution TOC-only articles (incl. Art 142), FALSE labels (`Section 1752`), SLGA 11/150 sections.
- 🧠 Reflection: two "failing tests" nobody had diagnosed were pointing at real content loss the whole time.

## Evaluation notes (flywheel)

- Failure modes observed: a psql `\\s` escaping bug briefly looked like OCR corruption — verified before reporting.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): re-index one document and diff the gap report.
