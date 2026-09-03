---
id: 0007
title: Admin console tabs + Build-Summaries UI + de-dup
stage: green
date: 2026-07-14
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [frontend, admin, ux, tabs, summaries, de-dup]
links:
  spec:
  ticket:
  adr:
  pr:
files:
  - src/pages/Admin.tsx
  - src/components/IndexedFolders.tsx
  - src/pages/SettingsPage.tsx
  - src/App.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit -p . (green)"
  - "npm run build (tsc && vite build, green)"
---

## Prompt

but there is no admin option to generate summaries no UI exist for this purpose, and one more ui issue for
admin is that all setting issues are collectively comes inside user management tab why? indexing, chunk
extraction, api conversion, traces, user files etc all are at one point it is not good also. and admin have
settings option from where we can extract traces and index files, again we have traces inside user management.
properly make them in seperate possible tabs for admin and dont duplicate them.

(Follow-up decisions: 4 tabs — Users · Indexing & Data · Model & Inference · Traces; strip Hardware from user
and admin views; remove the Settings surface from the admin experience, move necessary info into admin tabs.)

## Response snapshot

Turned the flat `/admin` page into a 4-tab Admin Console and gave the pipeline ops (incl. the missing
Build-Summaries button) a home; removed the duplicated trace/inference cards from the web Settings page.

- `pages/Admin.tsx`: `ADMIN_TABS` + `activeTab` signal; each former card wrapped in a per-tab `<Show>`.
  NEW "Derived layers (pipeline)" card → Build summaries / Rebuild graph / Link graph / Re-index structure via
  a shared `runPipeline()` POST helper. Renamed page to "Admin Console".
- `components/IndexedFolders.tsx` (NEW): folder manager extracted from SettingsPage, self-contained; used in
  both the Admin Indexing tab (web) and desktop Settings — no duplication.
- `pages/SettingsPage.tsx`: removed Hardware + GPU-Acceleration; folder section → `<IndexedFolders/>` (desktop
  only); Cloud Inference + Query Trace Logs gated to `isTauri()`; My Documents fetches its own folder list.
- `App.tsx`: hide Settings gear for web admins; menu "User Management" → "Admin Console".

Verified: `tsc --noEmit` and `npm run build` both green.

## Outcome

- ✅ Impact: admins can now (re)build summaries/graph from the UI; controls grouped into 4 tabs; traces/inference no longer duplicated across Admin + Settings; hardware cruft gone.
- 🧪 Tests: frontend typecheck + production build both pass. Runtime not yet exercised (not deployed).
- 📁 Files: Admin.tsx, IndexedFolders.tsx (new), SettingsPage.tsx, App.tsx, CHANGELOG_DEV.md.
- 🔁 Next prompts: deploy; then Indexing tab → Build summaries for constitution/Compendium/Manual → re-verify document_full rows.
- 🧠 Reflection: extracted the folder manager into a shared component rather than copy-pasting; kept desktop Settings intact so the change is web-focused and low-risk.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (build-verified only).
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after deploy, confirm the pipeline buttons hit the endpoints and the big-doc summary build actually produces rows.
