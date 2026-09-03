---
id: 168
title: Chat UI restyled to the supplied mockup — flat canvas, brand lockup, prompt cards
stage: refactor
date: 2026-08-25
surface: agent
model: claude-opus-5
feature: general
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (freeform)
labels: [ui, chat, branding, design-system]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/App.tsx
  - src/pages/ChatPage.tsx
  - src/pages/SearchPage.tsx
  - src/pages/Admin.tsx
  - src/components/DocumentTree.tsx
  - src/components/ProjectDocuments.tsx
  - src/components/ComingSoon.tsx
  - src/components/ChunkViewer.tsx
  - src/components/CitationFeedback.tsx
  - src/index.css
  - src/assets/noviq_mark.png
  - src/assets/noviq_logo.png
tests:
  - "npx tsc --noEmit — clean"
  - "npx vite build — clean"
  - "npx vitest run — 26/26 passed (2 files; no component coverage of these surfaces)"
---

## Prompt

now go to the downloads folder and view the file ui change image file, we want this type of ui and in the chat window dont write research station below the logo only logo is enough and make all backgrounds same and check other features also

## Response snapshot

Read `C:\Users\hp\Downloads\ui changes.jpeg`, compared it against the running
code, and asked two scoping questions (how far to take the mockup; whether the
mockup's "Repository" rail replaces the document tree). Architect chose **full
visual restyle** and **restyle the existing tree** (keep `DocumentTree` and its
scoping behaviour, give it the mockup's header).

Delivered:
- **One canvas tone.** `bg-noviq-panel` (#f3f6f2) removed from every chrome
  surface — top bar, both chat rails, their collapsed stand-ins, the Search page
  header strip, `ProjectDocuments` — all now `bg-noviq-paper`. Rail hover fills
  moved to `bg-noviq-50` (they were reading against the old tint and would have
  vanished). `index.css` body background was still painting the retired dark
  slate theme (#0f172a) with matching scrollbars; repainted to paper/ink.
- **Top bar**: brand lockup (mark + NOVIQ wordmark), icon+label nav, active tab
  underlined, height 12 → 14.
- **Chat hero**: logo only, **no "Research Station" tagline**; large headline;
  "TRY ASKING" rule; three titled icon cards (Procedure / Rules / Legal
  Guidance) carrying the existing example queries verbatim.
- **Right rail** is now "Repository" with `N Files · M Departments`, fed by a new
  `DocumentTree` `onStats` callback; the tree gained a department/document
  search box and its section label reads "Top departments".
- **Left rail**: the "Noviq / Research Station" text lockup became "Chat";
  "CHATS" → "Recent chats".
- **New assets** `noviq_mark.png` (256px) and `noviq_logo.png` (512px): the
  source `noviq_logo_3.png` is a 1.7 MB square with a baked-in white plate and a
  black bar on the bottom edge; at bar height its wordmark was illegible and its
  white background painted a visible box on the canvas.

## Outcome

- ✅ Impact: the chat surface matches the supplied design; the app no longer
  shows three different surface tones, and the last of the pre-Noviq dark theme
  is gone from `index.css`.
- 🧪 Tests: tsc clean, vite build clean, vitest 26/26. Rendered and screenshotted
  the real production bundle in desktop mode against no backend — chat, search
  and a ComingSoon page verified visually.
- 📁 Files: 10 modified, 2 new assets.
- 🔁 Next prompts: deploy and re-check the Repository rail with real data (the
  department rows and counts only appear with a corpus); decide whether the
  landing page keeps the 1.7 MB logo.
- 🧠 Reflection: the mockup itself contained the tagline the architect asked to
  remove — the instruction beat the picture. Worth remembering that a supplied
  design is a starting point, not the specification.

## Evaluation notes (flywheel)

- Failure modes observed: first pass used the original PNG in the new lockup and
  it rendered as a white box with a 6px illegible wordmark — caught only because
  the bundle was actually rendered, not because anything failed to compile.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): re-screenshot the Repository rail
  against the live corpus to confirm the department rows read as in the mockup.
