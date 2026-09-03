---
id: 0008
title: Remove the "Try the demo" button from the hero
stage: green
date: 2026-08-28
surface: agent
model: claude-opus-5
feature: 028-noviq-pk-cutover
branch: 028-noviq-pk-cutover
user: Riaz Mehmood
command: (conversational)
labels: [ui, landing, hero, deploy]
links:
  spec: specs/028-noviq-pk-cutover/
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src/components/LoginScreen.tsx
    reason: hero action row reduced to the single "Request a quote" call to action
tests:
  - name: npx tsc --noEmit
    result: pass
---

## Prompt

i want you to remove try the demo button from hero section

(then, after the change) deploy it

## Response snapshot

The hero's action row held two buttons: `Request a quote` (white, primary) and an
outline `Try the demo` button wired to `showPage("demo")`. Removed the button and
rewrote the neighbouring comment, which still asserted "Exactly two actions" — a
comment that contradicts the code is worse than no comment.

The demo is NOT gone from the site: the text link at `LoginScreen.tsx:745` and the
`/demo` route both still reach the same screen. Only the hero's second button was
removed, which is what was asked.

Deploy: the VPN gateway is down (4430 filtered, 443 open), so the scp + `npm run
build` frontend-only path to `<PRIVATE_IP>` is unreachable from Windows. Deployed
instead through the sanctioned auto-deploy path — push to `origin/release`, which
`noviq-deploy.timer` polls every ~2 min. The pending PHR/docs commit was batched
into the same push rather than pushed on its own, per the standing rule that a
docs-only push still costs a deploy cycle.

## Outcome

- ✅ Impact: the landing hero presents one action. Nothing else on the page changed.
- 🧪 Tests: `npx tsc --noEmit` clean; verified on the served bundle after deploy.
- 📁 Files: `src/components/LoginScreen.tsx`
- 🔁 Next prompts: none pending for the hero.
- 🧠 Reflection: the served-bundle grep must run BOTH directions — the hero string
  gone AND the nav link's lowercase "try the demo" still present. Checking only for
  absence would not distinguish "button removed" from "demo removed everywhere".

## Evaluation notes (flywheel)

- Failure modes observed: none in the edit; the deploy path had to change because the VPN is down.
- Graders run and results (PASS/FAIL): typecheck PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
