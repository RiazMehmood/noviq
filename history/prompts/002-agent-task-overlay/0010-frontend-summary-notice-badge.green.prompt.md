---
id: "0010"
title: Frontend summary notice badge
stage: green
date: 2026-05-02
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: master
user: Riaz Mehmood
command: Task 7 — Frontend summary notice badge
labels: ["frontend","solidjs","reviewer-agent","quality-notice","badge"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src/components/chat/QualityNoticeBanner.tsx
  - src/components/chat/MessageBubble.tsx
tests:
  - npx tsc --noEmit (0 errors)
---

## Prompt

You are implementing Task 7 of the Reviewer Agent feature for EchoAI (Tauri 2.0, SolidJS frontend).

**Task 7: Frontend summary notice badge**

Update two frontend files so that the blue info badge shows for summary quality notices (`constrained=false`), not just the yellow hardware warning (`constrained=true`).

**Files to modify:**
- `src/components/chat/QualityNoticeBanner.tsx`
- `src/components/chat/MessageBubble.tsx`

## Response snapshot

- Read both files; confirmed `dismissQualityNotice` from `../../stores/query` and `QualityNotice` from `../../api`.
- Replaced `QualityNoticeBanner.tsx` with dual-mode component: yellow (⚠) when `constrained=true`, blue (ℹ) when `constrained=false`.
- Changed `MessageBubble.tsx` line 30: `props.qualityNotice?.constrained` → `!!props.qualityNotice` so the banner renders for any non-null notice.
- TypeScript check: 0 errors.
- Committed: `db1b22eb feat(reviewer): summary info badge — blue QualityNoticeBanner for constrained=false`

## Outcome

- Impact: The blue info badge now renders for summary-mode responses (`constrained=false`), not just RAM-constrained hardware notices. Both yellow and blue paths are handled by a single component.
- Tests: TypeScript passes with 0 errors.
- Files: 2 files modified (QualityNoticeBanner.tsx, MessageBubble.tsx).
- Next prompts: Task 8 — Full test run + verification.
- Reflection: Simple, well-scoped change. Import paths confirmed from existing file before writing.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): tsc --noEmit PASS
- Prompt variant (if applicable): none
- Next experiment: verify in running app that blue badge appears on reviewer summary responses
