---
id: 112
title: Depth-first study notes rewrite
stage: explainer
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [teaching, ingestion, documentation]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- C:/Users/hp/Desktop/Noviq/Noviq-Pipeline-Study-Notes.md
tests:
- none (documentation only)
---

## Prompt

you are explaining good but the problem is missing pieces as in our previous
import input parameters, now again the python work, if you are telling about
the function like _page_without_superscript(page) so after this show its code
so it remain connected, it is already placed scattared in the code, so now i
want it in line to check their concurrency and working in line properly, if i
need to move there and there so what will be the purpose of these classes and
learning again i will go away from the main point, and teach me each and every
part of the code like teaching to a non technical person, so now update the
document completely from lesson one and go accordingly first function its
parameters and then first parameter how it is extracted whole its tree if it
is using a function show that function if it is getting another parameters
then go there show those till we get to the edge of that first parameter and
explain each code line how it is functioning.

## Response snapshot

Rewrote Noviq-Pipeline-Study-Notes.md completely in depth-first style: every
function's code appears inline at the moment it is named, followed by
line-by-line plain-language explanation, tracing each of import()'s five
parameters to its edge. Part 1 (raw_text) covers the full tree: extract()
ladder → discover()/run() subprocess plumbing → Python _op_extract_document →
_inner_gutters/_page_split/_column_consensus/_crop_preserves_anchors/_body_box
→ _superscript_keys/_page_without_superscripts → _provision_titles →
_extract_tables_for_page/_clean_cell → parse_document (form-feed join, title
folding, reflow) → text_quality::score (all 5 signals) → pdftotext/lopdf/OCR
tiers + final fallback. Parts 2-5: doc_title, force_profile
(get_doc_type_override), page_offsets (worked example), provision_titles
(cross-referenced). 8-question recap; deferred items listed explicitly.

## Outcome

- ✅ Impact: single linear read, no jumping; non-technical explanations per line.
- 🧪 Tests: n/a.
- 📁 Files: study notes fully rewritten (v2).
- 🔁 Next prompts: Lesson on text_normalizer::normalize in the same style.
- 🧠 Reflection: depth-first with inline code beats thematic summaries for this learner — keep this format for all future lessons.

## Evaluation notes (flywheel)

- Failure modes observed: earlier lessons referenced functions without showing bodies — the learner flagged it.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
