---
id: "015"
title: Fix LLM crash — logits flag and model path resolution
stage: green
date: 2026-04-15
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: fix LLM crash and get tier3 inference working
labels: ["llm","llama-cpp","crash","query-router","rag","fix"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/llm.rs
  - src-tauri/src/services/query_router.rs
tests:
  - cargo check --manifest-path src-tauri/Cargo.toml (passed, 10 warnings, 0 errors)
  - Manual E2E: query "definition of state" → tier3 LLM response confirmed
---

## Prompt

(Session 6 continuation) LLM still crashing on every query — `set_abort_callback: call` in logs,
app process killed. App was rebuilt after previous `logits=true` fix but fix was not yet tested.
User confirmed response was still not LLM output. Started monitor.py to trace every millisecond.

## Response snapshot

Root cause traced through monitor logs:

**Bug 1 — Wrong model path (resolved prior session)**
`resolve_model_path()` added to `query_router.rs` — walks up 6 directory levels from executable
to find `models/qwen2.5-3b-instruct-q4_k_m.gguf`. CWD of `target/debug/echo-ai` is `src-tauri/`
so relative path `models/...` was resolving wrong.

**Bug 2 — Stale cache (resolved prior session)**
All 19 entries in `query_cache` table had `tier_used = 'vec'` (old fallback answers). Cleared with
`DELETE FROM query_cache`.

**Bug 3 — Batch too large → OOM (resolved prior session)**
Changed `LlamaBatch::new(tokens.len(), 1)` to `LlamaBatch::new(n_prompt.max(1), 1)` with
`MAX_PROMPT_TOKENS = 512`. Context chunks reduced from 8 × 500 chars to 3 × 250 chars.

**Bug 4 — logits=false on all tokens → SIGABRT (fixed this session)**
All tokens were added to batch with `logits=false`. When `sampler.sample(&ctx, -1)` was called,
llama.cpp could not find any output position with logits computed → fired `set_abort_callback`
(native abort, uncatchable, kills entire process).

Fix in `llm.rs`:
```rust
for (i, t) in tokens.iter().take(n_prompt).enumerate() {
    let is_last = i == n_prompt - 1;  // logits=true ONLY for last token
    batch.add(*t, i as i32, &[0], is_last)?;
}
ctx.decode(&mut batch)?;
let mut output = String::new();
let mut i = n_prompt;  // was tokens.len() — wrong after truncation
```

**Confirmed working:** `set_abort_callback: call` in logs is NORMAL — it is llama-cpp-2
registering its abort callback during context initialization, not a crash. Context continues
loading (KV cache, sched_reserve) and inference proceeds.

## Outcome

- ✅ Impact: Tier 3 LLM inference confirmed working — query "definition of state" returned proper
  synthesized answer with citations, `tier3` label, 3B model on 16 GB RAM system.
- 🧪 Tests: `cargo check` passes (10 pre-existing warnings, 0 errors). Manual E2E query confirmed.
- 📁 Files: 2 files modified — `src-tauri/src/services/llm.rs`, `src-tauri/src/services/query_router.rs`
- 🔁 Next prompts: Second improvement item (user mentioned two things); E2E validation T059/T068/T077/T085
- 🧠 Reflection: The key insight was that llama.cpp requires `logits=true` on the last prefill token
  before any sampling can occur. Without this, `sampler.sample(&ctx, -1)` fires an uncatchable
  native abort. The `set_abort_callback: call` log line is initialization, not failure.

## Evaluation notes (flywheel)

- Failure modes observed: `set_abort_callback: call` misread as crash — it is normal init output;
  OOM on large batch fixed by capping at 512 tokens; wrong CWD fixed by resolve_model_path()
- Graders run and results (PASS/FAIL): cargo check PASS; manual E2E PASS (tier3 response confirmed)
- Prompt variant (if applicable): null
- Next experiment: Test remaining query types (analysis, knowledge graph) to validate tier routing
