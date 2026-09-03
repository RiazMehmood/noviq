---
id: 109
title: Login redirect race with Router mount
stage: green
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [web, auth, routing, bugfix]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src/components/LoginScreen.tsx
- CHANGELOG_DEV.md
tests:
- npx tsc --noEmit (pass)
---

## Prompt

new users are directed to admin console we already fix this so it redirect them to chat instead of admin.

## Response snapshot

Diagnosed a real race in the earlier fix (`1740a2ab`): the post-login
`replaceState(null, "", "/")` runs AFTER `login()` flips `currentUser`, which
synchronously mounts `<Router>` at the stale URL — so signing in from a copied
`/admin` link renders Admin despite the URL bar showing `/`. Drafted the fix
(move the URL reset before `await login()`), but the architect withdrew it:
the report was caused by sharing the `/admin` URL, and sharing the root URL is
the accepted resolution. Edit reverted; the latent race is recorded in
CHANGELOG_DEV.md as KNOWN LATENT with the one-line fix if ever wanted.

## Outcome

- ✅ Impact: no code change shipped (user decision); latent race documented.
- 🧪 Tests: `npx tsc --noEmit` was clean on the drafted fix before revert.
- 📁 Files: CHANGELOG_DEV.md (KNOWN LATENT entry); LoginScreen.tsx edit reverted.
- 🔁 Next prompts: share the root URL (not /admin) with new testers.
- 🧠 Reflection: replaceState after a reactive mount is invisible to the router — order matters, not just intent.

## Evaluation notes (flywheel)

- Failure modes observed: original fix verified only from "/" where it was a no-op.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
