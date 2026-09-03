---
id: 121
title: Tester decisions doc and instrument boost
stage: explainer
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [beta, feedback, retrieval, documentation]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- C:/Users/hp/Desktop/Noviq/Noviq-Beta-Update-For-Testers-2026-07-28.md
tests:
- none (documentation + explanation only)
---

## Prompt

before that we also make some decision share those also with them and what is
echo instrument boost? what is its purpose?

## Response snapshot

Explained ECHO_INSTRUMENT_BOOST from the actual code
(query_router.rs::apply_instrument_boost, :4944): post-retrieval graded
re-rank — when the query NAMES an instrument, chunks from the
filename-matching document get their RRF-style rank score multiplied
(full boost best match, proportional partial, untouched others); never a
filter, no-op when no law is named, typo-tolerant (trace bd0d16e1
"disciplane" test). Dormant since 2026-07-18 because the flag was never
added to the VM .env; still OFF. Wrote the tester-facing decisions doc
(Noviq-Beta-Update-For-Testers-2026-07-28.md): live changes (80/20 split,
6-char passwords, share root URL), decided items (display titles,
share-via-admin, instrument boost enable, citation download link), and
direct answers (physical vs printed page for Art 209 148/130 ≈ 18-page
front matter; /pN inherit defect acknowledged; "Schedule 6" mislabel =
compendium segmentation; unresolved = refuse-or-flag).

## Outcome

- ✅ Impact: shareable tester update ready; boost mechanism understood before enabling.
- 🧪 Tests: n/a.
- 📁 Files: one tester-facing md on Desktop\Noviq.
- 🔁 Next prompts: enable ECHO_INSTRUMENT_BOOST=2.0 on the VM when approved; /pN page fix.
- 🧠 Reflection: tester-facing docs must translate mechanisms into behaviour ("named law wins"), not knob names.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
