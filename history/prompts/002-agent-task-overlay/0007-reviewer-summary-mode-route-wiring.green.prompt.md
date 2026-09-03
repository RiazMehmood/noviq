---
id: "0007"
title: Reviewer Summary Mode Route Wiring
stage: green
date: 2026-05-02
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: master
user: Riaz Mehmood
command: inline
labels: ["reviewer-agent", "summary-mode", "query-router", "classify-query", "n_ctx", "system-prompt"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/query_router.rs
tests:
  - cargo check --no-default-features --features llm (Finished, no errors)
---

## Prompt

Task 4: Wire summary mode into route() — use classify_query() to select the correct prompt, max_tokens, n_ctx, and quality notice for each query. Detailed steps provided for: import update, early classification in route(), fetch_limit/build_context updates, build_rag_prompt new signature (system_prompt + focus_hint), try_llm_infer_streaming call update, summary QualityNotice insertion.

## Response snapshot

Applied all 7 steps to `src-tauri/src/services/query_router.rs`:

1. Updated import to include `SUMMARY_SYSTEM_PROMPT` and `SUMMARY_SYSTEM_PROMPT_VERSION`.
2. Added `classify_query()` call immediately after `let t_route =` in `route()`; destructured into `(system_prompt, gen_max_tokens, gen_n_ctx, gen_max_prompt, ctx_top_k, ctx_budget)` — Summary uses `(SUMMARY_SYSTEM_PROMPT, 512, 2048, 1400, 12, 640)`, Regular retains `(GROUNDING_SYSTEM_PROMPT, 256, 1024, 768, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET)`.
3. Replaced `CONTEXT_TOP_K * 2` with `ctx_top_k * 2` for `fetch_limit`.
4. Replaced `build_context(..., CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET)` with `build_context(..., ctx_top_k, ctx_budget)`.
5. Replaced `build_rag_prompt(query, context, history)` signature with new `(query, context, history, system_prompt: &str, focus_hint: bool)` — uses `question_line` format string; caller passes `focus_hint: false`.
6. Replaced hardcoded `256, 1024, 768` in `try_llm_infer_streaming` call with `gen_max_tokens, gen_n_ctx, gen_max_prompt`.
7. Added summary `QualityNotice` shadowing block after existing `effective_notice` — fires when `matches!(query_type, QueryType::Summary) && tier_used != "error"`.

`cargo check --no-default-features --features llm` → `Finished` with no errors.
Committed as `374d529e feat(reviewer): summary mode — SUMMARY_SYSTEM_PROMPT, n_ctx=2048, 512 tokens`.

## Outcome

- ✅ Impact: Summary queries now use dedicated system prompt, larger context window (n_ctx=2048), more tokens (512), wider retrieval (top-k=12, budget=640), and always receive a "General summary" quality notice.
- 🧪 Tests: cargo check clean; existing unit tests for classify_query, rrf_merge, build_context, validate_grounding all unaffected.
- 📁 Files: 1 file modified — query_router.rs (+47/-15 lines).
- 🔁 Next: Task 5 — judge_answer() function + tests.
