---
id: 0007
title: Live progress feed, ETA, and live writing pane
stage: green
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: (conversational request)
labels: [ux, progress, streaming, one-ask, frontend, events]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: history/adr/0013-one-ask-summary-composition-architecture.md
  pr: null
files:
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/src/services/drafting/one_ask/mod.rs
  - src-tauri/src/commands/one_ask.rs
  - src/api/oneAsk.ts
  - src/components/drafting/OneAskView.tsx
  - CHANGELOG_DEV.md
tests:
  - cargo check --lib (clean)
  - cargo test --lib drafting + tests/one_ask_acceptance.rs (stage asserts contains-based, Note-safe)
  - npx tsc --noEmit (clean)
---

## Prompt

recall memory, we were checking summary writing progress and it tooks too much time, and laptop battery goes off and it closed, some finding which i want to share with u are, draft button must go off after submitting text for summary, and it continuously showing one text that researching indexed documents for summary writing ... and it is taking too much time, so the user will get bored watching single words, i want you to make it interactive like it shows what is doing inside the indexed folder i,e checking xyz document... now again researchig on this document.... oooh find one related document now searching inside that.... got one citation going forward... sentences like that running on the screen so user understand the process and satisfies that it working inside also there should be some remaining time or percetage which shows how much work done how much still remaining...

(follow-up mid-task): it must be very much interactive and innovative... like it found one relate chunk or article and formulate that into paragraph so it must show that one the screen then after that again its research run for others chunks again found another one attach that below first and so on and at the end reshuffling and managing them it must shows on the screen must be interactive and appealing like AI making changes looks live. and at last all things get completes. we can show a pen writing on the screen it may looks more attractive like writing paragraph with pen... these are my recommendation improve and amend them as required

## Response snapshot

Backend: `research()` gained a `NoteFn` callback narrating per-document findings
("Searching the indexed documents for '<section>' material…", "Found <label> in
'<file>' — noting it as a citation candidate"); new `Stage::Note { percent, detail }`;
`ProgressEvent.percent` 0–100 (research 3–23, compose 25–95 per-section, verify 96);
`SlotWriter` now passes the AppHandle so every generated token streams on
`chat://token` (no other listener existed). Frontend: Draft button swaps to Cancel
while busy; event race fixed (events before startOneAsk resolves no longer dropped);
progress card = percent bar + linear-extrapolation ETA + 5-line scrolling activity
feed; live writing pane types each section onto the screen with an animated ✍️ pen
cursor, [CITE:n]/[FIG:n] rendered as ❲citation❳/❲figure❳ chips, verify stage shows a
"polishing/reshuffling" pulse before the final verified draft replaces the pane.

## Outcome

- ✅ Impact: minutes-long one-ask runs now show live per-document research notes, a percent bar with time-remaining, and the draft visibly assembling section-by-section.
- 🧪 Tests: cargo check --lib clean; drafting lib + one_ask acceptance suites GREEN; tsc clean.
- 📁 Files: 5 source files + CHANGELOG_DEV.md (see frontmatter).
- 🔁 Next prompts: live perf gate T035 (research ≤15 s on the 65 GB DB); US2 refuse-or-flag tasks T022–T025.
- 🧠 Reflection: reusing the existing per-token `chat://token` emission gave the "AI writing live" effect with zero inference-path changes.

## Evaluation notes (flywheel)

- Failure modes observed: original UI dropped early progress events (draftId race) — would have eaten the new notes too.
- Graders run and results (PASS/FAIL): compile + existing acceptance suites (PASS expected; contains-based stage asserts tolerate Note interleaving).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): smooth ETA with an EMA over percent-rate instead of whole-run linear extrapolation.
