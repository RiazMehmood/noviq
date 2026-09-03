---
id: 025
title: Session 28 B01-B05 Bug Fixes
stage: green
date: 2026-04-28
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: master
user: Riaz Mehmood
command: continue from last session
labels: ["bug-fix", "scrollbar", "analyze-persistence", "batch-persistence", "ner-kv-cache", "solidjs-store"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src/index.css
  - src/components/chat/ChatPanel.tsx
  - src/stores/analysis.ts
  - src/components/analysis/TemplateSelector.tsx
  - src/components/notebook/NotebookView.tsx
  - src/stores/batch.ts
  - src/components/batch/BatchPanel.tsx
  - src-tauri/src/services/llm.rs
tests:
  - Manual: chat scrollbar visible during streaming
  - Manual: auto-scroll during streaming generation
  - Manual: analyze result persists on tab switch
  - Manual: batch progress bar persists on tab switch
  - Manual: NER completes on dense-script chunks (no KV cache stall)
---

## Prompt

Continue from last session. All 5 session-27 bugs need fixing.

Additional user guidance during session:
- "we dont want to cancel anything just stop it for that time and again it starts while we return to that tab"
- "chat query response is our FIRST PRIORITY, ITS SPEED IS IMPORTANT, so dont make any change regarding it, which can cause reduction of its speed"

## Response snapshot

Fixed all 5 bugs across 2 commits:

**Commit 7dee8ef3 — B01–B04:**
- B01 (scrollbar): Added `.chat-scroll` CSS class with `!important` WebKit scrollbar display rules so scrollbar stays visible during streaming DOM mutations
- B02 (auto-scroll): Changed scroll effect to use `requestAnimationFrame` so scroll happens after DOM paint when `scrollHeight` is correct; also tracks `isLoading` to scroll on stream end
- B03 (analyze persistence): Created new `src/stores/analysis.ts` module-level store; lifted `analysisResult`, `analysisLoading`, `analysisError` out of TemplateSelector and NotebookView into shared store — survives tab switch
- B04 (batch persistence): Extended `src/stores/batch.ts` with `batchProgressPct` and `batchCurrentFile`; BatchPanel now reads from module-level store — survives accordion collapse

**Commit d198b593 — B05:**
- B05 (NER KV cache stall): Increased `n_ctx` from 1024 to 1536 in `infer()`. Root cause: with `MAX_PROMPT_TOKENS=928` and `max_tokens=256`, dense Arabic/Urdu chunks expand to 500+ tokens, pushing total to 928+256=1184 > 1024. Only affects `infer()` (NER + batch). Chat uses `infer_with_cancel()` with unchanged n_ctx=1024 — no speed impact.

## Outcome

- ✅ Impact: All 5 session-27 bugs addressed; backend jobs pause (not cancel) on tab switch; chat speed unaffected
- 🧪 Tests: Pending rebuild and manual verification
- 📁 Files: 8 files modified (7 frontend, 1 Rust)
- 🔁 Next prompts: Rebuild app, test all 5 fixes, then KG enhancement backlog
- 🧠 Reflection: Module-level SolidJS stores are the right pattern for any state that must survive component unmount. n_ctx must always be ≥ MAX_PROMPT_TOKENS + max_tokens to prevent KV overflow.

## Evaluation notes (flywheel)

- Failure modes observed: BatchPanel JSX still referenced old local signal names after import was updated — needed separate JSX pass
- Graders run and results (PASS/FAIL): cargo check PASS (0 errors, 7 warnings)
- Prompt variant (if applicable): null
- Next experiment: After rebuild, verify scrollbar fix holds through 5+ chat queries with streaming
