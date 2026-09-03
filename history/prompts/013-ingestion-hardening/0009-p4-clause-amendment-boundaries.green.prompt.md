---
id: 0009
title: Phase 6 — clause & amendment-bracket boundaries (D7/D8)
stage: green
date: 2026-06-20
surface: agent
model: claude-opus-4-8
feature: 013-ingestion-hardening
branch: 013-ingestion-hardening
user: engr.riaz2010@gmail.com
command: "go ahead with phase 6"
labels: [ingestion, structure-parser, clause-splitting, amendment-brackets, US4]
links:
  spec: specs/013-ingestion-hardening/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/structure_parser.rs
  - specs/013-ingestion-hardening/tasks.md
  - MODULAR_IMPROVEMENT_LOG.md
tests:
  - "cargo test --lib: 504 passed / 0 failed (+2 clause/bracket tests)"
---

## Prompt

go ahead with phase 6

## Response snapshot

Phase 6 = US4 (D7 sub-clause boundaries, D8 amendment brackets). Read the judge
audit + source layout: clauses are `\n\n`-separated but nested sub-clauses
`(3)(a)(b)(i)(ii)` are NOT, and an amendment-inserted sub-clause opens with `[`
(`2\n[(b) …`), so the blank-line splitter let `(a)` swallow the start of `(b)`.

Added `split_at_clause_markers` as step 0 of `split_long_text`: splits at
line-start `(2)(3)/(a)(b)/(i)(ii)` markers, tolerating a leading `[` so a
bracketed sub-clause opens its own piece. Pieces are contiguous slices ⇒
brackets/inserted text/footnote digit kept verbatim (G1 holds). Added 2 unit
tests. Full lib 504/0.

## Outcome

- ✅ Impact: sub-clauses split at correct boundaries (no swallow); amendment
  brackets preserved verbatim; legal clause structure now drives chunking.
- 🧪 Tests: full lib 504 pass / 0 fail (+2).
- 📁 Files: structure_parser.rs (splitter + tests), tasks.md, MODULAR_IMPROVEMENT_LOG.md.
- 🔁 Next prompts: Phase 7 (US5 amendment + cross-reference capture into graph).
- 🧠 Reflection: D7 and D8 are the same bug — the `[` that opens an amendment
  insertion is exactly the boundary the splitter was missing; one regex token
  (`\[?`) fixes both.

## Evaluation notes (flywheel)

- Failure modes observed: coalesce can re-merge sub-`min` marker pieces (accepted;
  only substantial-clause swallow is harmful).
- Graders run and results (PASS/FAIL): full lib gate PASS (504/0).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): re-index; count distinct sub-clause
  chunks per article vs source; confirm `(a)`/`(b)` are separate rows.
