---
id: 022
title: LLM Deep Profiling and Optimization
stage: green
date: 2026-04-25
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: calculate time and monitor processes until i told you to stop now first gather info as much as possible
labels: ["performance","profiling","llm","n_ctx","max_tokens","response-time"]
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

calculate time and monitor processes until i told you to stop now first gather info as much as possible so we can understand the behavior of app

## Response snapshot

Set up 3 concurrent monitors: live METRIC log watcher, system CPU/RAM sampler (10s), TaskBudget CPU watcher.
Analyzed 7 completed queries across 2 app sessions.

**Measured constants:**
- Prefill speed: 35 tok/s (consistent)
- Generation speed: 0.5-0.7 tok/s at 1536 tok KV; 1.4 tok/s at 768 tok KV
- Model load from NTFS: 3-25s variable
- KV cache per context: +151 MB RSS
- CPU during inference: 120-173% (1.2-1.7 of 4 logical cores)
- Idle eviction timer is a NO-OP (model stays loaded entire session)

**Bugs found and fixed:**
1. Token count loop was byte count → 28-min runaway
2. truncate() used byte slice → panic on UTF-8 multi-byte chars (em-dash)

**Optimizations applied (3 changes):**
- CONTEXT_TOKEN_BUDGET: 1536 → 768
- n_ctx/n_batch/MAX_PROMPT_TOKENS: 2048/2048/1536 → 1024/1024/768
- max_tokens: 256 → 96

All changes cargo-check clean.

## Outcome

- ✅ Impact: Predicted warm query time drops from 57-135s to 35-85s
- 🧪 Tests: cargo check PASS; runtime benchmark needed after restart
- 📁 Files: llm.rs (ctx_params ×2, MAX_PROMPT_TOKENS ×2), query_router.rs (CONTEXT_TOKEN_BUDGET, max_tokens)
- 🔁 Next prompts: restart app and benchmark; implement token streaming for UX
- 🧠 Reflection: Byte-count termination was the most critical hidden bug; KV cache size directly governs generation speed

## Evaluation notes (flywheel)

- Failure modes observed: byte-count runaway (28 min), UTF-8 panic on em-dash
- Graders run and results (PASS/FAIL): cargo check PASS
- Prompt variant (if applicable): null
- Next experiment: measure actual tok/s at new n_ctx=1024 vs predicted 1.4 tok/s
