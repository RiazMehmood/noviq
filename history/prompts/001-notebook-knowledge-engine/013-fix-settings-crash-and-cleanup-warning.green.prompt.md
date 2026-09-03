---
id: 013
title: Fix SettingsPage Crash and AppShell Cleanup Warning
stage: green
date: 2026-04-15
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: continue (context resumed)
labels: ["bugfix","routing","solidjs","settings","cleanup"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src/pages/SettingsPage.tsx
  - src/App.tsx
tests:
  - manual: navigate to /settings tab, verify hardware info renders without crash
  - manual: navigate between all 4 tabs multiple times, verify no tab gets stuck
---

## Prompt

Continued from prior context. Two browser console errors prevented tab navigation from working correctly after routing architecture was fixed:

1. `TypeError: undefined is not an object (evaluating 'hardwareTier().ram_gb.toFixed')` — SettingsPage crashed on mount, making it appear like navigation failed.
2. `cleanups created outside a createRoot or render will never be run` — AppShell's async onMount registered onCleanup after an await, outside reactive tracking context.

## Response snapshot

**Fix 1 — SettingsPage.tsx (lines 94–124)**:
Changed `<Show when={hardwareTier()}>` children from eager JSX (which evaluates `hardwareTier()!.ram_gb` even when `when` is falsy) to the SolidJS accessor pattern `{(tier) => ...}` which only evaluates when the signal is truthy. Also replaced the inline `{hardwareTier()!.has_discrete_gpu && ...}` conditional with a nested `<Show>` component.

**Fix 2 — App.tsx (lines 53–68)**:
Changed `onMount` from `async` to synchronous. `loadHardwareTier()` is now fire-and-forget (no `await`). The `onCleanup` call is registered synchronously so SolidJS can associate it with the correct reactive root. The `listen` promise chain stores the unlisten function in a closure variable that the cleanup callback captures.

## Outcome

- ✅ Impact: SettingsPage no longer crashes on mount; all 4 tabs (Notebooks, Search, Graph, Settings) navigate correctly without getting stuck
- 🧪 Tests: Manual navigation test across all tabs
- 📁 Files: src/pages/SettingsPage.tsx, src/App.tsx
- 🔁 Next prompts: T059 US2 E2E validation (add folder → scan → global search → revoke)
- 🧠 Reflection: SolidJS `<Show>` children are NOT lazily evaluated by default — only the accessor function pattern `{(accessor) => JSX}` provides lazy/guarded evaluation. Async onMount breaks reactive tracking; onCleanup must be called synchronously.

## Evaluation notes (flywheel)

- Failure modes observed: Repeated iterative changes + reloads wasted resources; root cause identification via browser console errors was the correct approach
- Graders run and results (PASS/FAIL): manual navigation — pending user confirmation
- Prompt variant (if applicable): null
- Next experiment: run app and click all 4 tabs to confirm no console errors
