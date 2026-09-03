---
id: 140
title: Spreadsheet compute path — the model plans, Rust computes
stage: green
date: 2026-08-03
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (conversational)
labels: [tabular, finance, determinism, adr-0019-pattern, eval]
links:
  spec: n/a
  ticket: n/a
  adr: ADR-0019 (same division of labour)
  pr: n/a
files:
  - src-tauri/src/services/tabular/mod.rs
  - src-tauri/src/services/tabular/sheet.rs
  - src-tauri/src/services/tabular/plan.rs
  - src-tauri/src/services/tabular/planner.rs
  - src-tauri/src/services/assistant/capabilities/tabular.rs
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/persona.rs
  - CHANGELOG_DEV.md
tests:
  - "21 tabular unit tests: pass"
  - "cargo check --lib / --bin noviq-server: clean"
  - "prod eval, 53 questions over 7 workbooks: 19/53 -> 49/53"
---

## Prompt

now build the compute path so these pass

## Response snapshot

Built `services/tabular/` on the ADR-0019 pattern — the model decides *what* to
compute, Rust computes it:

- `sheet.rs` — workbook to a **typed** table, keeping the types `extract_xlsx`
  throws away with `.to_string()`, and recovering numbers written as text
  (`Rs. 1,250.00`, `12.5%`, and accounting negatives where `(45.20)` is −45.20).
- `plan.rs` — a closed op set with filters and a row selector; the executor is
  pure, total and unit-tested against the exact figures prod got wrong.
- `planner.rs` — resolves ordinary questions to a plan with **no model at all**.
- `cross_foot` — the sheet's printed totals checked against its own rows.
- `capabilities/tabular.rs` — the lane, ahead of web and retrieval.

Measured every step against the same 53-question harness rather than assuming.

## Outcome

- ✅ Impact: **19/53 → 49/53** (50 correcting a grader artifact). All three
  original workbooks 10/10. The 240-row sheet 0/8 → 7/8 — the context overflow
  is gone because the model now receives only column names and 3 sample rows.
- 🧪 Tests: 21 unit tests; `cargo check --lib` and `--bin` clean; full prod eval.
- 📁 Files: 9.
- 🔁 Next prompts: the corpus join — an attachment-scoped turn cannot reach the
  indexed rules, so "which forum approves this project?" still fails.
- 🧠 Reflection: the first deploy moved the score 19 → 23 and I nearly reported
  that as progress. Instrumenting *why* (only 12 of 53 turns reached the lane)
  found the missing no-think prefix and, more importantly, showed the model was
  never needed for these shapes at all.

## Evaluation notes (flywheel)

- Failure modes observed: three defects that all produced **a wrong number
  presented as computed** — eager two-row-header detection swallowing a data
  row, trailing footnote rows counted into a total, and an unsupported date
  range silently dropped so a whole-sheet total was returned as if filtered.
  None were visible from unit tests written against clean fixtures; all three
  came from running the eval and reading the workings line.
- ⭐ Method that worked: making every answer print its own workings
  ("sum of Amount over 239 rows") — the row count is what exposed the
  off-by-one and the trailing-row over-count. A bare number would have hidden both.
- Graders run and results: 21/21 unit PASS; prod eval 49/53.
- Next experiment: date-range filters, and a ratio over a totals row.
