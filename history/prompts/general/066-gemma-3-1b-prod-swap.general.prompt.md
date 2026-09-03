---
id: 066
title: Gemma 3 1B prod swap with prompt-format seam
stage: general
date: 2026-07-06
surface: agent
model: claude-fable-5
feature: none
branch: release (via gemma-1b-integration)
user: Riaz Mehmood
command: conversation
labels: [vllm, gemma, model-swap, inference, prompt-format, deployment]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/src/services/inference/vllm.rs
  - deploy/vllm/run-vllm.sh
  - CHANGELOG_DEV.md
tests:
  - services::inference::vllm::tests::gemma_translation_folds_system_and_maps_roles
  - services::inference::vllm::tests::gemma_translation_preserves_multi_turn_history
  - services::inference::vllm::tests::gemma_translation_without_system_block_is_pure_role_mapping
  - services::inference::vllm::tests::gemma_stop_mapping_swaps_im_end
---

## Prompt

i want to use gemma model instead of qwen 3 8b, so for current architecture and for best results which model is most suitable list down them.

what is the context window of gemma 3 4b?

gemma 3 1b, and gemma 3 4b, compare them on our use case and also in context.

ok integrate gemma 3 1b.

(AskUserQuestion: "How should Gemma 3 1B be integrated?" → user chose "Replace Qwen3-8B in prod".)

## Response snapshot

Recommended Gemma 3 12B QAT as best fit (fits 12GB, runs on pinned vLLM 0.8.5); Gemma 4 needs
transformers ≥5.5 + nightly vLLM (blocked on this stack); advised 1B is below the quality floor
for citation-grounded legal answers (user's own 3B fine-tune eval invented a fake SPPRA rule).
User chose to replace Qwen3-8B with Gemma 3 1B in prod anyway.

Implementation: found all prompt builders emit hand-built ChatML sent raw to vLLM
`/v1/completions` — a bare swap would feed Gemma foreign control tokens. Smallest-diff fix:
`NOVIQ_VLLM_PROMPT_FORMAT=gemma` translation at the single `VllmGenerator::generate_stream`
choke point (system folded into first user turn, roles mapped, `<|im_end|>`→`<end_of_turn>`
prompt+stops, Qwen no-think prefill dropped). `run-vllm.sh`: `NOVIQ_VLLM_QUANT=none` skips
`--quantization`; repo copy reconciled to live fp16-KV/chunked-prefill (unit runs out-of-tree
`/opt/noviq/vllm-scripts/run-vllm.sh`, safe from `git reset` deploys). Weights:
`unsloth/gemma-3-1b-it` (google/ HF-gated → 403). VM compile check + 10/10 inference tests
green; merged `77d8f12a` to release; deploy + `.env` flip + restart + live-query verification.

## Outcome

- ✅ Impact: prod chat generator swapped Qwen3-8B-AWQ → Gemma 3 1B bf16 behind an env-selected
  prompt-format seam; default `chatml` keeps all other deployments byte-identical; revert is a
  three-key `.env` restore + service restart.
- 🧪 Tests: `cargo test --lib services::inference` 10/10 on VM (4 new Gemma translation tests).
- 📁 Files: vllm.rs (PromptFormat + chatml_to_gemma + stops_for_gemma), run-vllm.sh (repo +
  live VM copy, backup `.bak.20260706`), CHANGELOG_DEV.md.
- 🔁 Next prompts: A/B the 30-question eval set vs the 8B baseline; watch citation accuracy;
  revert if grounding regresses.
- 🧠 Reflection: read the seam before the swap — the "model swap" was actually a prompt-dialect
  problem; one choke-point translation beat rewriting five prompt builders.

## Evaluation notes (flywheel)

- Failure modes observed: gated HF repo (google/gemma-3-1b-it) → anonymous download 403; fell
  back to ungated unsloth mirror.
- Graders run and results (PASS/FAIL): VM cargo check PASS; inference unit tests 10/10 PASS;
  live-query smoke after cutover (see changelog).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): `tools/vllm_eval` 30-Q A/B Gemma-1B vs Qwen3-8B.
