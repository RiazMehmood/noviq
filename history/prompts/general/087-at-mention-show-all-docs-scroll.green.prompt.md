---
id: 087
title: @-mention picker — show all documents + scroll active row into view
stage: green
date: 2026-07-11
surface: agent
model: claude-opus-4-8
feature: chat composer UX
branch: release
user: Riaz Mehmood
command: (user bug report) while i write @ it only shows specific option not all documents and while i move down its scroll is not moving down
labels: [frontend, ux, chat, at-mention, bugfix]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src/components/ConfidentialComposer.tsx
  - CHANGELOG_DEV.md
tests:
  - frontend tsc --noEmit (PASS, exit 0)
---

## Prompt

while i write @ it only shows specific option not all documents and while i move down its scroll is not moving down it remains where it is

## Response snapshot

Traced the `@`-mention typeahead to `ConfidentialComposer.tsx`. Two bugs:
1. `mentionMatches` capped the list with `.slice(0, 8)` → only the first 8 of the full
   catalog ever showed. `docCatalog` (from `DocumentTree.onCatalog`) is already the complete
   flat list, so the cap was the sole cause. Raised to `.slice(0, 100)` (the dropdown scrolls
   via `max-h-56 overflow-y-auto`).
2. Keyboard ArrowDown updated `activeIdx` but never scrolled the highlighted row into view.
   Added a `mentionList` container ref + a `createEffect(activeIdx)` calling
   `row.scrollIntoView({ block: "nearest" })`.

Frontend `tsc --noEmit` clean. Frontend-only change → fast dist-only deploy, no backend restart.

## Outcome

- ✅ Impact: `@` now lists the whole library (scrollable), and keyboard navigation follows the
  highlighted row.
- 🧪 Tests: tsc PASS; manual click-through recommended after deploy.
- 📁 Files: `ConfidentialComposer.tsx` (+ CHANGELOG, this PHR).
- 🔁 Next prompts: resume Track 2 (verbatim synthesis).
- 🧠 Reflection: the container was already built to scroll; a stale item cap defeated it and
  masked the missing scroll-into-view until the list could exceed the viewport.

## Evaluation notes (flywheel)

- Failure modes observed: item-cap hiding data; keyboard nav without scroll-into-view.
- Graders run and results (PASS/FAIL): tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
