---
id: 059
title: Confidential-detail masking in chat (Phase 1)
stage: green
date: 2026-07-03
surface: agent
model: claude-opus-4-8
feature: general
branch: release
user: Riaz Mehmood
command: (chat) "start building phase 1" (confidential masking, F020)
labels: [chat, privacy, ui, masking]
links:
  spec: specs/020-confidential-masking/spec.md
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - src/lib/maskEngine.ts
  - src/components/ConfidentialComposer.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - tsc --noEmit exit 0
  - vite build exit 0
  - maskEngine round-trip (esbuild+node): 6/6 PII types detected, masked, refilled; "Rule 16(1)" preserved
---

## Prompt

"start building phase 1" — the confidential-detail masking feature (F020): highlight
private details as the officer types, let them include/exclude, and swap confidential
spans for reversible labels so the server/DB/logs only ever see labelled text.

## Response snapshot

Built Phase 1 (client-side only, no backend change):

- `maskEngine.ts` — regex detectors for CNIC / PK phone / money / file-case-no / email /
  date (legal tokens Rule/Article/Section deliberately excluded). `MaskSession` mints
  reversible labels (`[CNIC_1]`, same value → same label), `mask(raw, ranges)` splices
  right-to-left, `refill(text)` restores values, serialisable to sessionStorage.
- `ConfidentialComposer.tsx` — transparent textarea over a mirrored tinted highlight layer
  (a text box can't colour substrings); chip tray to keep/mask each detected item; "+ Mask
  selected" for manual marks; Enter submits.
- `ChatPage.tsx` — masks `input()` before `sendMessage` (server sees labels only); user
  bubble shows raw text; answers + live stream `refill()`-ed for display; per-conversation
  session-only map (`noviq_mask_<id>`), loaded on mount/newChat/selectConversation.

Verified: tsc + vite build green; engine round-trip proven via esbuild+node (all 6 PII
types, "Rule 16(1)" untouched). Deployed to `release` for browser testing.

## Outcome

- ✅ Impact: officers' PII no longer persisted to server/DB/logs; masked before send, refilled on screen.
- 🧪 Tests: tsc 0, vite build 0, engine round-trip PASS. Visual overlay needs browser eyeball.
- 📁 Files: maskEngine.ts (new), ConfidentialComposer.tsx (new), ChatPage.tsx (wire), CHANGELOG.
- 🔁 Next prompts: Phase 2 = names via GLiNER + POST /api/detect_pii; Phase 3 = mandatory-category policy + local-history hardening.
- 🧠 Reflection: overlay-highlight (mirror layer + transparent textarea) is the standard way to colour substrings a plain input can't; kept interactions in a chip tray so clicks don't fight the textarea.

## Evaluation notes (flywheel)

- Failure modes observed: `new Set()` inferred `Set<unknown>` (fixed to `new Set<string>()`).
- Graders run and results (PASS/FAIL): tsc PASS; vite build PASS; engine round-trip PASS.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): browser-verify overlay alignment + chip toggles + Mask-selected on a real query.
