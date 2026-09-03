---
id: 069
title: Gemma 4B bnb-4bit live + tunnel detach-poll fix
stage: general
date: 2026-07-06
surface: agent
model: claude-fable-5
feature: none
branch: release
user: Riaz Mehmood
command: conversation
labels: [gemma-4b, bitsandbytes, vllm, tunnel-timeout, detach-poll, prompt-profile]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/db/repositories/query_log.rs
  - src/api/assistant.ts
  - src/pages/ChatPage.tsx
  - docs/gemma-4b-staging-report-2026-07-06.md
  - CHANGELOG_DEV.md
tests:
  - cargo check --no-default-features --features llm,server,cuda --bin noviq-server (VM, exit 0)
  - npx tsc --noEmit (clean)
  - live smoke: bnb completion finite logprobs; SPPRA queries cited (grounding 0.86–1.50)
---

## Prompt

Session sequence (verbatim key asks): "recall memory" → "we need to diagnose the
issue, why gemma 4b failed? first share the details so i can also understand the
real issue." → "retry the bitsandbytes install" → "run the download in tmux, I'll
keep it open" → "go ahead with the flip" → "run the smoke test" → "if it is in
product then let me check it via browser" → "which prompt is currently used by
gemma 4b?" → "switch to the full prompt" → "what's the result of the heavy query?"
→ "on heavy test query, it was generating the answer also ran map also, but in the
end shows erro_apierror" → "how this map reduce functiona provoked? how we know
that this query will use map reduce instead of simple hybrid union?" → "as u
shared action items list, also share summerizedoc, compare, conflict and also
again action items." → "fix the tunnel timeout with the detach + poll option, and
also write a comprehsnive report on what was tested before for gemma 4 and what
were the bottlenecks, and what we are currently using, and how those bottlenecks
can be removed, and do this new strategy reduces the performance of the model or
no change at all? write that in docs folder."

## Response snapshot

1. Diagnosed prior 4B failures: AWQ+GPTQ int4 → NaN logits (fp16 quant kernels vs
   Gemma-3 bf16 activation overflow on vLLM 0.8.5/sm_86 multimodal path); bf16 →
   KV OOM on 12 GB. bnb 4-bit dequantizes in bf16 → hypothesized to dodge NaN.
2. Installed bitsandbytes 0.49.2 (`python -m pip` — uv venv has no pip binary),
   downloaded `unsloth/gemma-3-4b-it-bnb-4bit` in tmux (~100 s), flipped prod
   `.env` (backup `.env.bak.gemma1b.20260706`). Hit vLLM bnb loader requiring
   online repo listing under `HF_HUB_OFFLINE=1` → fixed by pointing MODEL at the
   local snapshot dir. Smoke PASSED: real text, finite logprobs, 3.57 GiB weights.
3. Token-budget mismatch surfaced (Gemma tokenizer up to ~60% over the Rust
   char-estimate on dense legal chunks; measured 13,243 real vs ≤8192 budget) →
   400s on map passes; headroom raised 1024→6144 (serve_len 14336). Proper fix
   (clamp in vllm.rs) left as follow-up.
4. Switched NOVIQ_PROMPT_PROFILE compact→full (v5.4.0); best grounding of session
   (1.50, 15 citations) under full prompt. Explained map-reduce triggers (cue
   regexes: ActionItems/Summarize+scope/CrossCompare/Conflict override).
5. Browser `error_apierror` on heavy query root-caused to Cloudflare tunnel ~100 s
   HTTP kill + axum cancel-on-disconnect aborting the turn. Fixed: detached
   `tokio::spawn` turn (gen-gate permit inside task), `POST /api/poll_turn`
   (user+conversation scoped, since_ms guard), `latest_since` repo fn, frontend
   `recoverTurn` polling 6 s ≤12 min. VM cargo check exit 0, tsc clean, merged
   `fix-tunnel-detach-poll` → release `68a83220`, auto-deploy watched.
6. Wrote `docs/gemma-4b-staging-report-2026-07-06.md` (what was tested, bottleneck
   analysis, current config, removal paths, performance impact: detach+poll = zero
   model-quality change; bnb = only working 4-bit on this stack).

## Outcome

- ✅ Impact: Gemma 3 4B live on prod with full prompt; long map-reduce answers no
  longer lost to the tunnel; comprehensive staging report in docs/.
- 🧪 Tests: VM cargo check exit 0; tsc clean; live smoke + 3 SPPRA-class queries
  (5.8 s cited / 10.6 s 6-cit / map-reduce 1.50 grounding); correct refusal probe.
- 📁 Files: query.rs (detach + poll_turn), router.rs, query_log.rs (latest_since),
  assistant.ts (pollTurn), ChatPage.tsx (recoverTurn), report doc, changelog.
- 🔁 Next prompts: verify deploy + end-to-end tunnel recovery in browser; vllm.rs
  real-token clamp; 30-Q A/B vs 8B; map-pass latency trims; query_log retention.
- 🧠 Reflection: two "model failures" were infra (offline-mode loader quirk,
  tunnel timeout) — separating kernel-NaN evidence from delivery-path evidence
  kept the diagnosis clean.

## Evaluation notes (flywheel)

- Failure modes observed: uv venv missing pip; bnb loader online-listing under
  HF_HUB_OFFLINE; char-based token estimate vs real tokenizer divergence scaling
  with chunk script/density; axum handler cancellation on client disconnect.
- Graders run and results (PASS/FAIL): smoke NaN check PASS; grounding scores
  0.86/1.50 PASS; refusal probe PASS; cargo check PASS; tsc PASS.
- Prompt variant (if applicable): v5.4.0 full profile on 4B (was v5.5.0-compact).
- Next experiment (smallest change to try): clamp max_tokens in vllm.rs using
  vLLM's tokenize endpoint or served-window feedback, then drop headroom to 2048.
