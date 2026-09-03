---
id: 060
title: Masking tray always visible
stage: green
date: 2026-07-04
surface: agent
model: claude-fable-5
feature: 020-confidential-masking
branch: release
user: Riaz Mehmood
command: chat
labels: [f020, masking, ux, frontend]
links:
  spec: specs/020-confidential-masking/spec.md
  ticket: null
  adr: null
  pr: null
files:
  - src/components/ConfidentialComposer.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit (exit 0)"
---

## Prompt

ok save this for future work, i want to work on masking work, it appear when it detects a word but i want to show it by default, so user have access to start masking at starting if he wants i want that.

## Response snapshot

Removed the `<Show when={chips().length > 0}>` gate around the confidential tray in
`ConfidentialComposer.tsx` so the tray (lock icon, chip row, "+ Mask selected" button)
renders from the first keystroke instead of appearing only after a regex detection fires.
Empty state shows "Masking on — detected details highlight here" as both status and
how-to hint; the "N will be masked" count replaces it once chips exist. No changes to
maskEngine or ChatPage. (Also saved the act/manual depth-audit findings to memory before
switching tasks.)

## Outcome

- ✅ Impact: users can manually mask ("+ Mask selected") before any auto-detection; masking is discoverable up-front.
- 🧪 Tests: `tsc --noEmit` clean; no frontend unit tests exist for components.
- 📁 Files: src/components/ConfidentialComposer.tsx (tray un-gated + empty-state label), CHANGELOG_DEV.md entry.
- 🔁 Next prompts: browser eyeball of tray spacing on empty composer; Phase 2b GLiNER names; Phase 3 local-history masking.
- 🧠 Reflection: the tray was designed detection-first; making it persistent needed only removing the gate plus an empty-state label — no engine change.

## Evaluation notes (flywheel)

- Failure modes observed: none — structural JSX edit, typecheck clean.
- Graders run and results (PASS/FAIL): tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): consider a compact single-line tray when composer is empty if vertical space bothers the user.
