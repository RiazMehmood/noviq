---
id: "0009"
title: "Retry Loop Route Wiring"
stage: green
date: "2026-05-02"
surface: agent
model: claude-sonnet-4-6
feature: 002-agent-task-overlay
branch: master
user: Riaz Mehmood
command: "Task 6: Wire the retry loop into route()"
labels: ["reviewer-agent", "retry-loop", "query-router", "rust"]
links:
  spec: "null"
  ticket: "null"
  adr: "null"
  pr: "null"
files:
  - src-tauri/src/services/query_router.rs
tests:
  - "cargo test --no-default-features --features llm --lib (55 passed)"
---

## Prompt

Task 6: Wire the retry loop into route() in query_router.rs.

Add RETRY_PARAMS constant and a judge + retry loop block into route() in query_router.rs.
Steps: (1) Read file to understand exact variable names and structure. (2) Add RETRY_PARAMS constant after CONTEXT_TOKEN_BUDGET. (3) Insert retry loop after grounding validation block, before effective_notice. (4) Run cargo check and fix errors. (5) Run all backend tests. (6) Commit.

## Response

Added RETRY_PARAMS constant immediately after CONTEXT_TOKEN_BUDGET (line 26):
- `const RETRY_PARAMS: [(usize, usize); 3] = [(10, 560), (12, 640), (12, 640)];`

Inserted retry loop block (80 lines) after the grounding validation closing `};`, before the `// Surface only chunks` comment. Key implementation details:
- Loop condition: skip when `was_cancelled || tier_used == "error" || answer == GROUNDING_FALLBACK`
- Calls `judge_answer()` first; PASS breaks immediately; FAIL escalates
- After 3 retries exhausted: returns error QueryResponse with tier_used="error"
- Each retry fetches wider context (escalating k and budget from RETRY_PARAMS)
- Retry calls `build_rag_prompt(..., focus_hint: true)` for emphasis
- Grounding re-validated on each retry answer
- Tuple `(answer, tier_used)` shadows previous bindings (intentional Rust shadowing)
- `was_cancelled` correctly wired from initial try_llm_infer_streaming result
- All free functions (fts5_retrieve, rrf_merge, build_context, etc.) called directly

Result: `cargo check` clean (Finished in 8.72s), 55 unit tests pass, committed as e0e1aa7f.

## Outcome

- RETRY_PARAMS constant added after CONTEXT_TOKEN_BUDGET
- Retry loop inserted after grounding validation, before effective_notice
- Loop uses judge_answer() — PASS breaks, FAIL escalates
- After 3 failed retries: returns error QueryResponse with tier_used="error"
- Retry calls build_rag_prompt with focus_hint=true
- cargo check passes (0 errors, 0 warnings)
- 55 unit tests pass; 1 pre-existing integration test failure (BackendAlreadyInitialized — unrelated)
- Committed: e0e1aa7f "feat(reviewer): retry loop — up to 3 retries with escalating context + error response"
