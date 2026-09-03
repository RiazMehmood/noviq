---
id: 113
title: Teaching lesson page number extraction
stage: explainer
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [teaching, ingestion, page-numbers]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- C:/Users/hp/Desktop/Noviq/Noviq-Pipeline-Study-Notes.md
tests:
- none (teaching/documentation only)
---

## Prompt

i want details how page numbers are extracted from the document so with
examples show that then i will move to other nomalization techniques.

## Response snapshot

Appended LESSON 2 (page numbers end to end) to the study notes, depth-first
with inline code and worked byte-level examples: (2.0) text has no page
numbers — physical pages vs printed footers; (2.1) the form feed \x0c and its
three birthplaces; (2.2) pdftotext_page_offsets with a 3-page byte-table
example; (2.3) page_for walk-through; (2.4) the RC-PG drift bug — why import()
recounts after normalization (SPPRA Rule 17 p13→p5), with a before/after map
example; (2.5) the invariant that every line-deleting stage saves the line's
form feeds — all three keepers' code (chrome, front_matter,
strip_table_regions) + the two 2026-07-27 defects (header ate the break; table
spanning a break, 238→149 pages, 341 chunks on p137); (2.6)
cleaned_page_offsets map translation with example; (2.7) page_by_path final
stamping; (2.8) one-screen journey diagram; 8-question recap.

## Outcome

- ✅ Impact: page numbering fully covered before moving to normalization.
- 🧪 Tests: n/a.
- 📁 Files: study notes appended (Lesson 2).
- 🔁 Next prompts: text_normalizer::normalize repair passes.
- 🧠 Reflection: byte-table worked examples suit this learner; keep numbers small and ASCII.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
