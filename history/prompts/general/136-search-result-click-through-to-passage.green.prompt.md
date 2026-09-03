---
id: 136
title: Search result click-through to passage
stage: green
date: 2026-07-31
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform)
labels: [frontend, search, solidjs, silent-failure, deploy, cherry-pick]
links:
  spec: N/A
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - src/pages/SearchPage.tsx
  - src/components/search/SearchResultCard.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit (release tree): PASS"
  - "npx vite build (release tree): PASS"
  - "deployed bundle contains the source_chunks call: PASS (dist rebuilt 20:33, HEAD 0656531)"
  - "user-confirmed working in the browser: PASS"
---

## Prompt

now it is working fine but it is not leting user go to that document, like in
citation.

ok now it is working fine, save this progress upto now.

## Response snapshot

**Two silent failures stacked**, which is why the click appeared to do nothing at
all rather than fail visibly:

1. `SearchPage` passed an `onClick` that only `console.log`-ed the file path — a
   T055 placeholder (*"Future: open source in notebook view or file explorer"*)
   that shipped and was never revisited.
2. `SearchResultCard.handleOpen` then called `openInDefaultApp`, which imports
   `@tauri-apps/plugin-shell` to hand the path to the OS. That module does not
   exist in a browser, so it threw into a `console.warn` no user sees.

The same silent-`catch{}` shape as the clipboard-to-Word bug. Fixed both halves:
the desktop call is gated behind `isTauri()` (a browser has no business opening
OS file paths), and the card now opens **`ChunkViewer`** — the same drawer a
clicked citation opens — with `focusChunkId` set to the matching chunk, so a
search hit and a citation land the reader at the same passage in the same UI.

`audience="user"` is passed explicitly and is **required**: the prop defaults to
`"admin"` (corpus-wide console view) which would 403 an officer. The user path
reads `/source_chunks`, scoped through `visible_source_ids` — the same invariant
as the new `/api/global_search`, so anything a user can find they can also open
and nothing beyond it.

Cherry-picked to `release` as `0656531`; frontend-only (`FE=1, BE=0`), no rebuild
of the backend and no restart. Docs kept on `release-deploy` (`ddfdfe9e`).

## Outcome

- ✅ Impact: Global Search is now end-to-end usable on the web — it has a route,
  it is visibility-scoped, and a hit opens the passage. **User-confirmed working.**
- 🧪 Tests: tsc + build clean on the release tree; served bundle verified to carry
  the `source_chunks` call; confirmed in the browser by the architect.
- 📁 Files: 2 source + changelog.
- 🔁 Next prompts: consider whether Search should use hybrid FTS+vector like
  `query_router` rather than pure vector — as shipped, Search and Chat can rank
  the same words differently.
- 🧠 Reflection: the useful pattern this session was refusing to accept "nothing
  happened" as a symptom. Both halves of this bug were failures that *logged* and
  then swallowed — one to `console.log`, one to `console.warn`. Neither was
  visible from the UI, and the placeholder had survived because a stub that logs
  looks like working code in a diff.

## Evaluation notes (flywheel)

- Failure modes observed: none new here. The cherry-pick was clean because the
  earlier lesson was applied first — `git fetch` the VM's `release` and diff
  against deployed HEAD *before* starting. Also hit a small process snag: an
  uncommitted `CHANGELOG_DEV.md` blocked `git checkout release`, so docs must be
  committed (on the dev line) before switching branches to cherry-pick.
- Graders run and results (PASS/FAIL): tsc PASS, build PASS, bundle check PASS,
  user acceptance PASS.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): grep the frontend for surviving
  placeholder handlers (`console.log` inside an `onClick`) — this one shipped and
  sat there, and a second may well exist.
