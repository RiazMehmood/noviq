---
id: 123
title: Split child pages and citation download
stage: green
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [citations, pages, tester-feedback, privacy]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src-tauri/src/services/structured_importer.rs
- src-tauri/src/api/documents.rs
- src-tauri/src/api/router.rs
- src/api/transport.ts
- src/components/AnswerBody.tsx
- src/components/ChunkViewer.tsx
- CHANGELOG_DEV.md
tests:
- split_children_stamp_their_own_page_not_the_parents (new regression test)
- npx tsc --noEmit (clean)
---

## Prompt

5 and 10 only all others are not required.

(mid-turn) and download link will download what? clear this also?

## Response snapshot

Item 5 — split-chunk pages: `collect_chunks` resolves a page as
`page_by_path.get(unit_path).or(parent_page)`, and `finalize_tree`'s `/pN`
(paragraph split) and `/mN` (merged scrap) nodes were never in the map, so
each piece of a page-spanning section claimed the section's first page.
Added `stamp_split_child_offsets`: document-order walk with a forward-only
cursor, locating each unmapped node by a 48-char prefix of its own text
(only the head is verbatim after trimming/joining); unlocatable nodes keep
the inherit fallback. Proven by the map in the first (red) test run:
`Rule_2 → 1` while `Rule_2/m3, /m4 → 2`. The first test assertion was wrong
(filtered `/p` when the coalescer had produced `/m`); corrected to match on
the parent prefix.

Item 10 — clarified with the architect that the link opens the ORIGINAL
indexed file, and went further than a plain download: `GET
/api/sources/:id/file` serves it INLINE (scoped through `visible_source_ids`,
404 not 403) and the FE opens it in a new tab at `#page=N` via
`openAuthedInNewTab` (bearer auth rules out `target="_blank"`; blob URL
revoked after 60 s). Buttons in the citation popover and the source viewer;
pop-up-blocked and fetch errors surfaced, never swallowed.

## Outcome

- ✅ Impact: page-accurate citations for split sections; one-click page verification.
- 🧪 Tests: new regression test + tsc clean; VM run in progress at write time.
- 📁 Files: 6 modified (see list).
- 🔁 Next prompts: deploy to release; browser-test the new tab + share queue.
- 🧠 Reflection: the red test run's debug map proved the FIX while failing the ASSERTION — print the state you are asserting on.

## Evaluation notes (flywheel)

- Failure modes observed: assumed `/pN` survives finalize_tree; the coalescer renames to `/mN`.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
