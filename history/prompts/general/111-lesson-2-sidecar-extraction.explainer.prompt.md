---
id: 111
title: Teaching lesson 2 sidecar extraction
stage: explainer
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [teaching, ingestion, sidecar, pdfplumber]
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

text, rows, tables, provision_titles how we extract these thing from raw pdf
text which we get from pdf plumber? especially provision titles?

## Response snapshot

Appended Lesson 2 to the study notes: the pdfplumber sidecar
(src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py) is a one-shot
subprocess speaking JSON over stdin/stdout (_op_extract_document, py:476).
Per-page order: superscript-digit filter (54.→154. defect) → catchline titles
matched on the filtered page BEFORE cropping → crop to body column →
extract_text(layout=True) → tables from the RAW page. provision_titles =
document-level gutter consensus (~40-page sample, median gutter, ≥50% support,
vetoed by _crop_preserves_anchors when cropping would lose provision numbers —
the hanging-indent case), then pure vertical pairing: each provision owns
[top−6pt, next top) and margin lines in that band join hyphen-aware into its
title. rows are made in Rust (reflow_with_sidecar) from the Python tables.

## Outcome

- ✅ Impact: Lesson 2 appended with self-test; study file now covers Lessons 1-2.
- 🧪 Tests: n/a.
- 📁 Files: Desktop\Noviq\Noviq-Pipeline-Study-Notes.md (appended).
- 🔁 Next prompts: Lesson 3 — text_normalizer::normalize.
- 🧠 Reflection: the per-page ORDER of operations (filter→titles→crop→text) is itself the key teaching point.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
