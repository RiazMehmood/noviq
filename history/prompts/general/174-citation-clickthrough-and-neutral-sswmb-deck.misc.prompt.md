---
id: 174
title: Citation click-through fixes and the neutral SSWMB deck
stage: misc
date: 2026-08-30
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (chat) rebuild the deck without advocacy; fix the confidence badge; make citations openable
labels: [presentation, sswmb, citations, f027-projects, deploy]
links:
  spec: specs/027-my-projects/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/pages/ChatPage.tsx
  - src/components/AnswerBody.tsx
  - src/components/ChunkViewer.tsx
  - src/components/ConfidentialComposer.tsx
  - CHANGELOG_DEV.md
  - "Desktop/Noviq/Solid Waste Management/md briefing.pptx"
tests:
  - "npx tsc --noEmit: PASS (each change)"
  - "live behaviour on noviq.pk: masking off, badges, Sources click-through, Open document — all verified"
---

## Prompt

(Continuation of PHR 173. Successive requests in one session.)

1. "presentation is ok now" → then: rebuild with NO proposal, NO statutory
   appointment routes, NO references to their cases. Slide 1 = Noviq logo +
   Riaz Mehmood (Town Officer BPS-16) and Mohammad Younus Shaikh (Municipal
   Officer BPS-17). Remove old slide 2. On the question slides "just show what
   noviq shows in results only. it is for them they will automatically
   recongnize this".
2. "yes fix the confidence badge"
3. "there is no clickable citation in the answer … so how we can go to the
   document then?"
4. "yes do the clickable sources fix and deploy"
5. "unable to open the document"
6. "now it is opening the document i checked that"

## Response snapshot

**Deck rewritten neutral — 18 slides.** All advocacy removed; verified by
rendering every slide via PowerPoint COM (`SaveAs($dir, 18)`) and scanning the
saved file's text for forbidden terms (court, petition, press, donor, chairmen,
deputation, S.12(1)(n), pilot, "we are asking") — none present. Auto-sizing
headings to one line fixed five layout collisions only visible in the render.

**Four fixes shipped and verified live** (bundles `BLCDj8Yy` → `BcpLzKcO` →
`DSZRKbqR` → `DY3Vhrth` → `ClO38IdI`):

- `a20511f7` masking opt-in, off by default.
- `72283a7a` uncited answers now read `N RELATED PASSAGES · CONFIDENCE:
  UNVERIFIED` + amber banner; cited ones still read `CITED 6 TIMES · HIGH`.
  `modelCited()` re-derives from `[N]` markers because `TurnResponse` carries no
  `quality_notice` — the server's own warning is dropped before the client.
- `cf839d60` the Sources list is clickable (was plain `<li>`, never wired up).
- `eb2d7538` + `4365a6d1` **citation opening never worked inside a project**:
  `source_chunks`/`source_file` scope through `scope_for(project_id)` and project
  scope is exclusive, so without it a project document 404s. `projectId` threaded
  ChatPage → AnswerBody → ChunkViewer; `project_id` (snake) in the query string
  for the GET route. Took two passes — the first missed `ChunkViewer`'s own
  "Open document ↗".

## Outcome

- ✅ Impact: the deck is presentable; citation verification works in projects for
  the first time; an unverified answer no longer claims high confidence.
- 🧪 Tests: typecheck clean on each change; every fix verified by behaviour on
  the live site, and the architect confirmed the final one himself.
- 📁 Files: as listed above.
- 🔁 Next prompts: the pop-up race in `openAuthedInNewTab`; plumb
  `quality_notice` into `TurnResponse`; SEPA 2014 missing from a Sources list
  that names it.
- 🧠 Reflection: I fixed a call site instead of a route and shipped it as done.
  Grepping every caller of `/sources/{id}/file` would have caught the second one
  before the architect did.

## Evaluation notes (flywheel)

- Failure modes observed: single-call-site fix presented as complete; a missing
  scope parameter failing CLOSED and therefore silently; `window.open` after an
  `await` losing the user gesture.
- Graders run and results (PASS/FAIL): all live checks PASS at session end.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): open the tab synchronously on click,
  then set `win.location` once the blob resolves.
