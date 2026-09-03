---
id: 0012
title: One-ask draft wiped on tab navigation — state moved to module store
stage: green
date: 2026-06-15
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: (chat) bug report — drafting UI wipes on tab navigation
labels: [bugfix, frontend, solidjs, drafting, state-persistence]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket:
  adr:
  pr:
files:
  - src/stores/oneAsk.ts (new — module-level run-state store + reducers + one-time listener init)
  - src/stores/oneAsk.test.ts (new — 4 DOM-free regression tests)
  - src/components/drafting/OneAskView.tsx (read from store; drop local run-state + listener teardown)
  - CHANGELOG_DEV.md (dated entry)
tests:
  - "vitest src/stores/oneAsk.test.ts --environment node: 4/4 PASS"
  - "tsc --noEmit: exit 0"
---

## Prompt

"but there is another issue also as we move to other tabs while writing summary, and
returned back so each and everything wipes out it is not persisting it is running in
background but not showing in the ui."

(Follow-up clarified scope: data is already in the DB as draft/chat history; user chose
"just the wipe fix" rather than also building a draft-history browse/edit feature.)

## Response snapshot

Systematic debugging. Root cause traced through four boundaries: `OneAskView` kept all run
state in component-local signals and tore down its three Tauri listeners in `onCleanup`; the
SolidJS router unmounts the Drafting page on tab switch, destroying state and listeners, so
`drafting://one-ask-progress|complete` + `chat://token` events fired while away were dropped.
The backend job (`start_one_ask` detached spawn keyed by draft_id, `state::save` persistence)
is decoupled from the webview — hence "running in background." Same class previously fixed for
Chat (module store).

Fix (frontend-only): new `stores/oneAsk.ts` holds run state at module scope + pure reducers +
idempotent module-scope listener registration (dynamic-imported api so the module loads under
Node test runner). `OneAskView` now reads the store and only keeps UI-transient local signals.

## Outcome

- ✅ Impact: live draft (progress, writing pane, activity feed, finished view) survives tab
  navigation; events keep landing in the store while away because listeners are never torn down.
- 🧪 Tests: `oneAsk.test.ts` 4/4 (node env); `tsc --noEmit` clean. Live GUI tab-nav check pending
  (app/dev-server were down).
- 📁 Files: 2 new (store + test), 1 edited (OneAskView), 1 changelog.
- 🔁 Next prompts: live verify on relaunch; optional SDD spec for draft history (expose
  `state::list`, history UI, reopen/edit) — data already persisted (mig 023/025).
- 🧠 Reflection: the existing `stores/drafting.ts` header ("module-level signals survive tab nav")
  documented the right pattern; the 010 surface simply never adopted it.

## Evaluation notes (flywheel)

- Failure modes observed: component-local state + listener teardown on a router-unmounted page.
- Graders run and results (PASS/FAIL): store regression tests PASS; tsc PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live tab-nav verification with the dev server up.
