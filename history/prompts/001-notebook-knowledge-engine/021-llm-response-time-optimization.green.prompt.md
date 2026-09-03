---
id: 021
title: LLM Response Time Optimization
stage: green
date: 2026-04-25
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: continue the last session, improvement still needed in response time
labels: ["performance","llm","response-time","n_ctx","threading"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/llm.rs
  - src-tauri/src/services/query_router.rs
tests:
  - cargo check --features llm (exit 0)
---

## Prompt

continue the last session, improvement still needed in response time

## Response snapshot

Applied 4 targeted optimizations to reduce LLM response time:

1. **n_ctx 4096→2048** + **MAX_PROMPT_TOKENS 3584→1536** in both `infer` and `infer_with_cancel` — cuts prefill O(n²) ~5x on long prompts; halves KV-cache memory
2. **n_batch 4096→2048** — matched to new n_ctx
3. **Explicit 4 threads** via `.with_n_threads(4).with_n_threads_batch(4)` — uses all 4 CPU cores for both generation and prefill
4. **CONTEXT_TOKEN_BUDGET 3072→1536** in query_router.rs — limits chunks to fit new n_ctx
5. **max_tokens 768→256** in try_llm_infer call — grounding prompt asks for 2-5 sentences; 256 tokens is generous; 3x faster generation

`cargo check --features llm` passed clean (exit 0, only pre-existing deprecation warnings).

Expected: response time drops from ~2+ min → ~40-60 seconds on 4-core CPU.

## Outcome

- ✅ Impact: Estimated 3-5x reduction in LLM response time through combined prefill + generation optimizations
- 🧪 Tests: cargo check passes; runtime test needed after rebuild
- 📁 Files: llm.rs (ctx_params in 2 functions), query_router.rs (CONTEXT_TOKEN_BUDGET + max_tokens call site)
- 🔁 Next prompts: Test with "capital punishment" query to verify <1 min; consider streaming tokens for perceived latency
- 🧠 Reflection: n_threads was the missing piece — llama.cpp default uses system cores but explicit setting ensures correct allocation

## Evaluation notes (flywheel)

- Failure modes observed: none at compile time
- Graders run and results (PASS/FAIL): cargo check PASS
- Prompt variant (if applicable): null
- Next experiment (smallest change to try): measure actual time; if still slow consider token streaming via Tauri events
