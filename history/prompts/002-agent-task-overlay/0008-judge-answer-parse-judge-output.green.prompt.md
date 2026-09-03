---
id: "0008"
title: "judge_answer() + parse_judge_output() — LLM YES/NO classification"
stage: green
date: "2026-05-02"
surface: agent
model: claude-sonnet-4-6
feature: "002-agent-task-overlay"
branch: master
user: "Riaz Mehmood"
command: "Task 5: judge_answer() function + tests"
labels: ["reviewer-agent", "query-router", "llm-judge", "tdd", "rust"]
links:
  spec: "null"
  ticket: "null"
  adr: "null"
  pr: "null"
files:
  - "src-tauri/src/services/query_router.rs"
tests:
  - "services::query_router::tests::judge_yes_response_passes"
  - "services::query_router::tests::judge_no_response_fails"
  - "services::query_router::tests::judge_empty_or_unclear_fails"
---

## Prompt

Task 5 of the Reviewer Agent feature for EchoAI (Tauri 2.0 Rust app).

**Task 5: judge_answer() function + tests**

Add `parse_judge_output()`, `build_judge_prompt()`, and `judge_answer()` to `query_router.rs`.

Steps:
1. Write failing tests (judge_ prefix tests were already present in the file)
2. Run tests to verify failure — `parse_judge_output` not defined
3. Add the three functions after `try_llm_infer_streaming`
4. Run tests to verify all 3 pass
5. cargo check
6. Commit

## Response

The judge tests were already present in the test block at the bottom of `query_router.rs` (lines 1352–1374). Confirmed they failed with `error[E0425]: cannot find function parse_judge_output in this scope`.

Added three functions at line 828 (after `try_llm_infer_streaming`):

- `parse_judge_output(output: &str) -> bool` — returns true only when output contains "YES" (case-insensitive via `.to_uppercase()`)
- `build_judge_prompt(query, answer) -> String` — minimal ChatML prompt, YES/NO only, ~100 tokens
- `judge_answer(model_slot, model_path, query, answer, tier) -> bool` — async, spawn_blocking, max_tokens=5, n_ctx=512, fail-open on error

All 3 judge tests passed: `judge_yes_response_passes`, `judge_no_response_fails`, `judge_empty_or_unclear_fails`.

Cargo check: clean (3 dead_code warnings expected — functions wired in Task 6).

Commit: `19212e71` — `feat(reviewer): judge_answer() + parse_judge_output() — LLM YES/NO classification`

## Outcome

- Red → Green: tests moved from compile error to 3/3 passing
- No regressions in the 51 other tests
- Dead_code warnings are acceptable — Task 6 (retry loop) will wire these in
