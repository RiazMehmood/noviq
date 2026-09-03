---
id: 008
title: Fix llm.rs build errors — llama-cpp-2 0.1.143 API
stage: green
date: 2026-04-14
surface: agent
model: claude-sonnet-4-6
feature: 001-notebook-knowledge-engine
branch: 001-notebook-knowledge-engine
user: Riaz Mehmood
command: continue where we left
labels: ["rust", "llm", "build-fix", "llama-cpp-2", "toolchain"]
links:
  spec: null
  ticket: null
  adr: null
  pr: null
files:
  - src-tauri/src/services/llm.rs
  - src-tauri/rust-toolchain.toml
tests:
  - cargo check --features llm (passes clean)
---

## Prompt

continue where we left

## Response snapshot

Fixed all blocking `cargo check --features llm` errors in `src-tauri/src/services/llm.rs` for llama-cpp-2 0.1.143:

1. Added `use llama_cpp_2::sampling::LlamaSampler;` import.
2. Changed `with_n_gpu_layers(gpu: i32)` → `with_n_gpu_layers(gpu: u32)`.
3. Replaced old sampling API (`candidates_ith`, `LlamaTokenDataArray::from_iter`, `ctx.sample_temp/top_p/sample_token`) with `LlamaSampler::chain_simple([LlamaSampler::temp(0.7), LlamaSampler::greedy()])` + `mut sampler.sample(&ctx, -1)`.
4. Changed `with_n_ctx(NonZeroU32)` → `with_n_ctx(Some(NonZeroU32))` (API now takes Option).
5. Changed `token_to_str(token)` → `token_to_str(token, Special::Plaintext)` (second arg required in 0.1.143).
6. Fixed Cargo.lock version 4 → 3 (system Rust 1.75 doesn't support v4).
7. Created `rust-toolchain.toml` pinning to `stable` so rustup 1.94.1 is used instead of system 1.75.

## Outcome

- ✅ Impact: `cargo check --features llm` finishes clean (Finished dev in 6.95s, 0 errors)
- 🧪 Tests: `cargo check --features llm` — passes
- 📁 Files: llm.rs (fixed), rust-toolchain.toml (new)
- 🔁 Next prompts: Run full `npm run tauri build` (release build, currently in progress); then integration tests
- 🧠 Reflection: llama-cpp-2 API changed significantly between 0.1.x versions — sampler chain replaces inline ctx methods; `with_n_ctx` now takes `Option`; `token_to_str` needs `Special` enum

## Evaluation notes (flywheel)

- Failure modes observed: Multiple minor API signature mismatches discovered iteratively via `cargo check`; rustup toolchain PATH shadowing by system Rust was unexpected
- Graders run and results (PASS/FAIL): cargo check --features llm → PASS
- Prompt variant (if applicable): null
- Next experiment: Full release build via `npm run tauri build`
