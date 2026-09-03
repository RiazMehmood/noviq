---
id: 0015
title: Phase 7 — Noviq drafting UI + citation hover-to-verify
stage: green
date: 2026-06-08
surface: agent
model: claude-opus-4-8
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: "complete phase 7 of 009 feature ui work"
labels: [frontend, solidjs, ui, drafting, citation-grade]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket:
  adr: history/adr/0008-deterministic-citation-integrity-for-answers-and-drafts.md
  pr:
files:
  - src/api/drafting.ts
  - src/stores/drafting.ts
  - src/pages/Drafting.tsx
  - src/components/drafting/IntentForm.tsx
  - src/components/drafting/DocSet.tsx
  - src/components/drafting/GapPanel.tsx
  - src/components/drafting/DraftView.tsx
  - src/components/drafting/TemplateManager.tsx
  - src/App.tsx
  - src/assets/noviq_logo_3.png
  - src/vite-env.d.ts
  - tailwind.config.js
tests:
  - "tsc --noEmit: PASS (0 errors)"
  - "vite build: PASS (77 modules, built in 27.5s)"
---

## Prompt

complete phase 7 of 009 feature ui work of noviq

and i also want that if we hover over cited number it must shows original context of that citation in a pop up or cloud what so ever fits so we can verify that it is giving correct answers.

## Response snapshot

Built the full Phase-7 UI wave for Feature 009 (backend was already GREEN, headless):
branding→Noviq + green `noviq` Tailwind palette; `src/api/drafting.ts` (typed wrappers for all 13
drafting commands) + `src/stores/drafting.ts` (flow signals); `src/pages/Drafting.tsx` 3-column
research-station shell with Index-Folder-first empty state and a live "Research Insights" rail; and
the per-story components IntentForm/DocSet/GapPanel/DraftView/TemplateManager driving
start_draft → update_doc_set → render_draft → revise_draft → finalize_draft → export_draft.
Added the requested verification affordance: `CitationChip` in DraftView shows a hover/focus popover
with the ORIGINAL source context (verbatim chunk + source id/page) for every cited/fact block — no
backend change needed since `Block::Cited.quote` / `Block::Fact.value` already carry the full chunk
and `citation`/`attribution` carry the source+page. Marked T003/T004/T005/T012/T021/T027/T031/T035/
T036/T037/T048 done; appended a CHANGELOG_DEV entry.

## Outcome

- ✅ Impact: The citation-grade drafting pipeline is now usable end-to-end from the UI; officers can
  index a corpus, set up templates, curate a doc set, render, verify each citation against its source
  on hover, revise iteratively, finalize and export.
- 🧪 Tests: `tsc --noEmit` clean; `vite build` clean. Backend untouched (no Rust rebuild needed).
- 📁 Files: 12 created/modified (8 new TSX/TS, App.tsx + tailwind.config.js edits, logo asset, vite-env).
- 🔁 Next prompts: wire prose model slot (T017) so prose blocks aren't placeholders; T038/T039 polish;
  manual `npm run tauri dev` smoke test of the live flow with real Sindh corpus.
- 🧠 Reflection: The hover-to-verify requirement landed for free because the backend already returns
  verbatim chunks — confirming the value of the extractive, deterministic citation design (ADR-0008).

## Evaluation notes (flywheel)

- Failure modes observed: none at build time; runtime flow not yet smoke-tested in a live Tauri window.
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): launch `npm run tauri dev`, index the Sindh corpus folder,
  run one Summary draft through to export, and confirm the citation popover shows faithful source text.
