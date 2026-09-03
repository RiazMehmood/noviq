---
id: 110
title: Teaching lesson 1 import inputs
stage: explainer
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [teaching, ingestion, structured-importer]
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

now you are teacher you will teach me step by step the whole extraction pipe
line. as u recently created the file noviq import pipeline code walkthrough so
now teach me all it step by step. first let us discuss about the import
function it get in his input these values raw_text: &str, doc_title: &str,
force_profile: Option<&str>, page_offsets: &[(u32, usize)], provision_titles:
&std::collections::HashMap<String, String>, now show me code of them each one
by one, and also write a file for that as we are discussing here in modular
steps i want to go through the whole pipeline, as we are discussing we will
also update that file accordingly so for my recall i will use that file
understood?

## Response snapshot

Started a running study file `Desktop\Noviq\Noviq-Pipeline-Study-Notes.md`
(Lesson 1). Traced each of import()'s five inputs to its true origin: raw_text
= ParsedDocument.text from the 4-tier quality-gated extractor ladder
(pdfplumber sidecar → pdftotext → lopdf → OCR, pdf.rs:26); doc_title = bare
file name (indexer.rs:729); force_profile = sources.doc_type WHERE
doc_type_source='user' (sources.rs:312); page_offsets = form-feed scan
(pdf.rs:236), recomputed inside import() after normalization; provision_titles
= catchline column paired in the pdfplumber sidecar (only place with
y-geometry), folded per-document in table_sidecar.rs:171. Lesson ends with a
6-question self-test. Next lesson: text_normalizer::normalize.

## Outcome

- ✅ Impact: study file started; will be appended lesson-by-lesson.
- 🧪 Tests: n/a.
- 📁 Files: Desktop\Noviq\Noviq-Pipeline-Study-Notes.md (new).
- 🔁 Next prompts: Lesson 2 — normalize() repair passes.
- 🧠 Reflection: single-caller functions teach best by starting from the call site.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
