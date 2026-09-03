---
id: 0003
title: Parameterise infer_with_cancel n_ctx + max_prompt_tokens
stage: green
date: 2026-05-02
surface: agent
model: claude-sonnet-4-6
feature: none
branch: master
user: Riaz Mehmood
command: Task 3: Parameterise infer_with_cancel (n_ctx + max_prompt_tokens)
labels: ["implementation","rust","refactor","llm","signature-change"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/llm.rs
  - src-tauri/src/services/query_router.rs
tests:
  - cargo check --no-default-features --features llm
---

## Prompt

You are implementing Task 3: Parameterise infer_with_cancel (n_ctx + max_prompt_tokens)

Currently `infer_with_cancel` hardcodes `n_ctx=1024` and `MAX_PROMPT_TOKENS=928`. Add `n_ctx: u32` and `max_prompt_tokens: usize` as explicit parameters so callers can control them. Three call sites need updating.

Files to modify:
- src-tauri/src/services/llm.rs — both stub and full implementations
- src-tauri/src/services/query_router.rs — try_llm_infer_streaming function and its call site in route()

Steps:
1. Update stub infer_with_cancel in llm.rs (add _n_ctx: u32, _max_prompt_tokens: usize params)
2. Update full infer_with_cancel in llm.rs (replace hardcoded 1024 and 928 with params)
3. Update try_llm_infer_streaming in query_router.rs (add n_ctx, max_prompt_tokens to signature + pass through)
4. Update call site in route() to pass 256, 1024, 768 as defaults
5. cargo check — verify zero warnings and errors
6. Commit

## Response

All 4 edits applied cleanly across 2 files:

1. Stub `infer_with_cancel` in `#[cfg(not(feature = "llm"))]` impl: added `_n_ctx: u32` and `_max_prompt_tokens: usize` parameters.
2. Full `infer_with_cancel` in `#[cfg(feature = "llm")] mod full`: replaced hardcoded `1024` n_ctx with `n_ctx_nz = NonZeroU32::new(n_ctx.max(64))` and replaced `MAX_PROMPT_TOKENS = 928` const with the `max_prompt_tokens` parameter.
3. `try_llm_infer_streaming` in query_router.rs: added `n_ctx: u32` and `max_prompt_tokens: usize` to signature; passed through to `infer_with_cancel` call.
4. Call site in `route()`: updated from `(model_slot, &model_path, &prompt, 256, tier, cancel, app)` to `(model_slot, &model_path, &prompt, 256, 1024, 768, tier, cancel, app)`.

cargo check: `Finished` with zero warnings and zero errors.
Commit SHA: a4626dd7
