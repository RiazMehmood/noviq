---
id: 068
title: Gemma 4B staging failed session save
stage: general
date: 2026-07-06
surface: agent
model: claude-fable-5
feature: none
branch: release
user: Riaz Mehmood
command: conversation
labels: [gemma, vllm, model-staging, nan-logits, ops]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - CHANGELOG_DEV.md
tests:
  - live smoke: /v1/completions logprobs probe (NaN evidence)
  - live smoke: restored 1B /api/query answers with 3 citations
---

## Prompt

now what are other options exist? is there any gemma 3 2b model or gemma 3 4b?

ok stage 4B,

did gptq work? test the queries

did bf16 work? test the queries

but as per expert guidance they said that, it can fit with little changes and must occupy less
space than qwen 3 8b, so what changes we need to make?

save progress upto now we start a new session.

## Response snapshot

No Gemma 3 2B exists (family = 1/4/12/27B). Staged 4B three ways on the prod VM; all failed on
vLLM 0.8.5 + sm_86 + cuda-compat: QAT-AWQ and GPTQ/compressed-tensors both load but emit empty
text with NaN logprobs (4B multimodal Gemma3ForConditionalGeneration; text-only 1B unaffected);
bf16 (8.1GB) KV-OOM crash-loops even at util 0.92/7168 window/4 seqs. Experts' size claim is
correct (int4 ~3GB < 8B-AWQ ~5.7GB) — the blocker is broken int4 kernels for this arch on the
pinned old vLLM, not fit. Prepped the one untried little change (bitsandbytes 4-bit: venv pip
bootstrapped, live run-vllm.sh handles QUANT=bitsandbytes) — resume steps in memory
project_gemma-1b-swap.md. Session ended on user request: prod RESTORED to Gemma 3 1B (compact
profile) and verified live.

## Outcome

- ✅ Impact: 4B ruled out on current stack with evidence (3 variants); prod restored + verified;
  bnb-4bit resume path fully staged.
- 🧪 Tests: NaN logprobs probe; restored-1B live query (3 citations).
- 📁 Files: CHANGELOG_DEV.md (4B findings entry); memory files updated.
- 🔁 Next prompts: bnb-4bit attempt (steps in memory); if NaN again → Qwen3-4B-AWQ or vLLM
  upgrade decision; sync bnb line into repo deploy/vllm/run-vllm.sh.
- 🧠 Reflection: "fits on paper" ≠ "runs on stack" — quant-kernel/arch support on a pinned old
  vLLM is the real gate; probe logprobs early to distinguish NaN kernels from prompt bugs.

## Evaluation notes (flywheel)

- Failure modes observed: NaN logits (awq_marlin + compressed-tensors on multimodal Gemma3),
  KV-OOM on bf16, quant-flag/config mismatch (GPTQ repo actually compressed-tensors), uv venv
  without pip.
- Graders run and results (PASS/FAIL): 4B FAIL x3; 1B restore PASS.
- Prompt variant (if applicable): n/a (ops session).
- Next experiment (smallest change to try): QUANT=bitsandbytes + unsloth/gemma-3-4b-it-bnb-4bit.
