---
id: 061
title: Sign-in always lands on Chat
stage: green
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: general
branch: release
user: Riaz Mehmood
command: chat
labels: [web, auth, routing, ux]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src/components/LoginScreen.tsx
- CHANGELOG_DEV.md
tests:
- npx tsc --noEmit (exit 0)
---

## Prompt

before this integration there is one issue, as user or admin login it redirected them directly to the setting page, so admin can view the user management pane but normal user gets error of authentication this is not good, every sign in redirected to the chat section only then they can navigate to others. first fix this then we come to our current fixtures.

## Response snapshot

Root cause: `LoginScreen` is a full-screen overlay rendered OUTSIDE the SolidJS Router
(App.tsx auth gate), so the browser URL stays whatever it was pre-login (e.g. `/settings`
from a stale session, bookmark, or refresh). After auth flips, the Router mounts at that
leftover path — normal users stranded on Settings hit admin-gated fetches → "authentication
error".

Fix (smallest diff): in `LoginScreen.handleSubmit`, after `login()` succeeds, call
`window.history.replaceState(null, "", "/")` before the app shell mounts, so the Router
always starts at `/` (Chat). `replaceState` (not push) so Back doesn't return to the stale
path. `restoreSession` deliberately untouched — an already signed-in user refreshing on
/settings should stay there.

Shipped: `1740a2ab` pushed to `release` (VM auto-deploys). CHANGELOG_DEV entry added.

## Outcome

- ✅ Impact: every fresh sign-in (admin or user) lands on Chat; no more 403s for normal users on Settings.
- 🧪 Tests: `tsc --noEmit` exit 0; browser verify = log out on /settings → sign in as normal user → lands on Chat.
- 📁 Files: src/components/LoginScreen.tsx (+7), CHANGELOG_DEV.md (+15).
- 🔁 Next prompts: user-perspective powers — retrieval owner-scoping fix, user upload UI, share toggle, chat attachments.
- 🧠 Reflection: the bug was a router-mount-order artifact, not a navigate() call — no redirect code existed at all; the "redirect to settings" was inherited URL state.

## Evaluation notes (flywheel)

- Failure modes observed: none — one-line fix, typecheck green.
- Graders run and results (PASS/FAIL): tsc PASS; browser verification pending user.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): if users report losing deep links, gate the reset to non-"/" paths only for role!=admin.
