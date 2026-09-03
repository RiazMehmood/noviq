---
id: 0014
title: Fix admin couldn't see deep toggle (duplicate /me race)
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (freeform) "i logged in as admin but unable to view deep search button why?"
labels: [deep-search, gating, bugfix, auth, single-source-of-truth]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a (direct fast-forward push to release, 4c055961)
files:
  - src/pages/ChatPage.tsx
  - src/stores/auth.ts
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: clean; vite build: clean"
  - "live: admin /me {role:admin, deep_search_enabled:true}; user /me false (server correct all along)"
  - "UI user-verified (4c055961): admin sees Deep-search toggle, non-admin does not"
---

## Prompt

i logged in as admin but unable to view deep search button why?
(then: "{role:\"admin\", deep_search_enabled:true} but ui is not showing deep search option")
(then: "checked for both user and admin now it is working fine for admin only")

## Response snapshot

Debugged without guessing. Ruled out caching first (served bundle was fresh + contained the gate), then
confirmed the server was correct — admin `/me` returned `{role:"admin", deep_search_enabled:true}`. So the bug
was purely frontend. (One self-inflicted detour: my first console `/me` snippet used the wrong localStorage key
`token` instead of `noviq:authToken`, producing a misleading 401 — corrected and called out.)

Root cause: `ChatPage.tsx` ran its **own** second `GET /me` in `onMount` to populate a local `isAdmin` + the new
`deepEnabled` signal, inside a silent `.catch(()=>{})`. That page-level fetch is independent of the canonical
auth boot; any hiccup left `deepEnabled=false` permanently with no error, hiding both the toggle and the admin
Debug link (same call gated both). Meanwhile the rest of the app (`App.tsx`, `SettingsPage`, `IndexedFolders`)
already reads admin state from the one canonical store `stores/auth`. ChatPage was the lone outlier duplicating
session/role resolution.

Fix (frontend only): derive from the canonical, boot-validated store instead of a duplicate fetch.
- `stores/auth.ts`: `deep_search_enabled?` added to `CurrentUser` (already returned by `/me`; `login` +
  `restoreSession` capture it); exported `deepSearchEnabled()` from `currentUser`.
- `ChatPage.tsx`: deleted the local `isAdmin`/`deepEnabled` signals AND the duplicate `onMount` `/me`; imports
  `isAdmin` + `deepSearchEnabled` from the store. `App` renders ChatPage only after `currentUser()` is set
  (`needsLogin` gate) → value present before first paint, no race. Added a `createEffect` forcing `deepSearch`
  off when not enabled; removed the now-dead `request` import.

Deployed `4c055961`; user confirmed in the real UI: works for admin only.

## Outcome

- ✅ Impact: the admin-only Deep-search gate is now actually usable — admins see the toggle, users don't,
  end-to-end in the browser. Fixed a latent class of bug (per-page duplicate `/me`).
- 🧪 Tests: tsc + vite build clean; server /me both roles; UI user-verified.
- 📁 Files: ChatPage.tsx (−duplicate /me, −2 signals, −dead import, +store derivation +reset effect),
  stores/auth.ts (+field +helper), CHANGELOG.
- 🔁 Next prompts: the deep-search smoothing backlog (garble tail, appeal-confusion, resume dead-code) — now
  safely admin-only. Also the web-build Tauri console leaks remain parked.
- 🧠 Reflection: the fix wasn't "make the second /me work" — it was deleting the second /me. A duplicated
  source of truth for session/role is the bug; the swallowed catch made it invisible. Same silent-fallback
  lesson as the clipboard fix earlier today.

## Evaluation notes (flywheel)

- Failure modes observed: (1) my diagnostic snippet used the wrong token key → false 401, briefly misleading;
  (2) the real defect was masked by a silent `.catch(()=>{})` — undebuggable by design, like the clipboard case.
- Graders run and results (PASS/FAIL): tsc PASS; vite build PASS; /me admin PASS; /me user PASS; UI admin-only
  PASS (user-confirmed).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): audit for any other per-page `/me` or role fetch that should read
  the canonical store instead (grep `"/me"` across src/ — expect only stores/auth now).
