---
id: 153
title: GPU inference decode throughput root cause
stage: explainer
date: 2026-08-12
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (freeform research request)
labels: [performance, vllm, gpu, awq, marlin, inference, diagnosis]
links:
  spec: specs/016-vllm-serving/
  ticket: n/a
  adr: history/adr/0018-server-side-inference-engine-vllm-continuous-batching-sidecar.md
  pr: n/a
files:
  - path: deploy/vllm/run-vllm.sh
    note: "line 30 — QUANT default 'awq' is the defect; should be awq_marlin (not yet changed)"
  - path: src-tauri/src/services/query_router.rs
    note: "lines 2006/2154 — t_llm spans prefill+decode only, not retrieval (verified, no change)"
tests:
  - name: live decode benchmark (single stream, 100/300 tok)
    result: "13.76 / 13.64 tok/s"
  - name: live concurrency scaling (1/4/8 streams)
    result: "13.7 / 13.1 / 13.0 tok/s per-stream; 104 tok/s aggregate at 8"
  - name: live prefill benchmark (unique prompts, prefix cache defeated)
    result: "5611 / 4952 / 4735 tok/s => 77.6-92.0 TFLOPS"
  - name: device-to-device bandwidth benchmark in serving venv
    result: "564 GB/s (81% of A40 spec 696)"
  - name: vLLM /metrics production accumulators (50 real requests)
    result: "TPOT 74.45ms (13.43 tok/s), TTFT 661ms, e2e 36.2s"
---

## Prompt

why our gpu inference of llm is so slow? what is current tokens/sec rate and why, and how can we optimize this? first research deeply one by one each and everything then conclude, no guesses no assumptions, search the web for possible speed on existing system and then also see why we are not achieving that?

## Response snapshot

**Rate: 13.4 tok/s decode**, which is 98.5% of a 36.2 s average answer.
Source: vLLM `/metrics` accumulators over 50 real production requests
(`time_per_output_token_seconds` sum/count = 74.45 ms). Arithmetic closes:
479 output tokens x 74.45 ms + 0.66 s TTFT = 36.3 s vs measured 36.2 s e2e.

**Cause: `--quantization awq` forces the unoptimised AWQ kernel.** vLLM says so
itself in the journal of the live process (PID 2408312, started 07 Aug):

    INFO [awq_marlin.py:117] Detected that the model can run with awq_marlin,
    however you specified quantization=awq explicitly, so forcing awq.
    Use quantization=awq_marlin for faster inference
    WARNING [config.py:830] awq quantization is not fully optimized yet.

Flag originates at `deploy/vllm/run-vllm.sh:30` (default `awq`), set live as
`NOVIQ_VLLM_QUANT=awq` in `/opt/noviq/echoAI/.env`.

**The hardware is not the limit — three measurements locate the fault:**

| Path | Measured | vs hardware | Verdict |
|---|---|---|---|
| Prefill compute | 4,735 tok/s @ 8,707 tok | 77.6 TFLOPS = **52% of full A40 FP16 peak** | healthy |
| Raw memory bandwidth | d2d copy benchmark | **564 GB/s = 81% of A40 spec (696)** | healthy |
| Decode | 13.7 tok/s x 6.10 GB weights | 84 GB/s = **15% of the 564 available** | **broken** |

Memory-bound decode ceiling at 564 GB/s is **92 tok/s**; we get 13.7 → **6.8x headroom**.

**Eliminated, each by measurement:**
1. Long prompts — 14-tok prompt decodes at 13.76, a 4,423-tok prompt at 13.43. Flat across 300x context.
2. Retrieval/prefill — 1.8% of latency; prefill 4,700–5,600 tok/s on unique text.
3. Saturated GPU — 8 concurrent streams held 13.0 tok/s each (104 aggregate, 8x scale for 5% cost). A saturated device cannot do that.
4. **The "time-sliced 1/4 GPU" story — FALSE.** A40-12Q partitions *frame buffer* (12 of 48 GB), not compute. Measuring 52–61% of the full card's FP16 peak and 81% of its bandwidth is impossible under a 25% compute cap.
5. CUDA compat shim / eager mode — prefill traverses the same R470 shim and is healthy; CUDA graphs confirmed captured (`enforce_eager=False`).

**Fix, ranked:** (1) `NOVIQ_VLLM_QUANT=awq_marlin` + fix the `run-vllm.sh:30` default — expect 40–90 tok/s; (2) n-gram speculative decoding (our answers quote retrieved passages verbatim — ideal case), +1.5–2.5x on top; (3) shorter answers, pro-rata; (4) bigger GPU — low value while we use 15% of the one we own.

Latency for the average 479-token answer: **36.4 s today → 12.7 s @ 40 → 8.7 s @ 60 → 6.0 s @ 90.** F016's SC-002 gate (p95 <= 10 s), written off as unreachable, returns to range.

Report artifact: https://claude.ai/code/artifact/687b60fa-ee1a-41cf-8fba-e5275876858c

## Applied and verified (same session)

`NOVIQ_VLLM_QUANT=awq_marlin` set in `/opt/noviq/echoAI/.env` (backup `.env.bak.marlin`);
`noviq-vllm` restarted 2026-08-12 19:09, healthy after 177 s. vLLM now logs
`The model is convertible to awq_marlin during runtime. Using awq_marlin kernel.`

| Metric | Before | After | |
|---|---|---|---|
| Decode, 300 tok | 13.75 tok/s | **81.32 tok/s** | **5.9x** |
| Production-shaped (4.4k prompt, 479 out) | 36.2 s | **7.7 s** | **4.7x** |
| Decode bandwidth utilisation | 15% | **88%** of achievable | near-optimal |
| Prefill, 8.7k tok | 4,804 tok/s | 3,516 tok/s | **27% slower — accepted** |
| VRAM free | 1,400 MiB | 1,998 MiB | improved |
| KV cache | 14,800 tok | 13,968 tok | concurrency 1.61x -> 1.52x |

**F016 SC-002 (p95 <= 10 s), written off as unreachable on this hardware, now passes
single-stream.** Prefill regression is absorbed because prefill is <2% of latency.

### Then: n-gram speculative decoding (flagged, default OFF in repo, ON in prod)

New `NOVIQ_VLLM_SPEC=ngram` knob -> `--speculative-config '{"method":"ngram",...}'`.
Draft-model-free, output bit-identical (rejection sampling), no extra VRAM.

| | spec OFF | spec ON |
|---|---|---|
| 1 user, RAG shape | 84.8 tok/s | **168.9-181.0** (2.1x) |
| 1 user, quote-heavy / free prose | 83.8 / 83.0 | 133.8 / 113.3 |
| **8 users, aggregate** | **596.8** | **461.4 (-23%)** |
| 8 users, slowest request | **4.02 s** | 5.20 s |

Forces the V0 engine, but **chunked prefill survives** (verified `chunked_prefill_enabled=True`)
so the F016 §7 two-wave pathology does not return. Architect chose ON: prod is ~15 req/day
with `num_requests_running` never above 1, and the bad case still lands inside SC-002.
**Reset: `NOVIQ_VLLM_SPEC=` empty + restart.**

**Cumulative: production-shaped request 36.2 s -> 7.7 s -> 3.6 s ~= 10x.**

### ⚠️ Correction issued mid-session

`deploy/vllm/run-vllm.sh` is **OUT OF TREE** — the unit runs
`/opt/noviq/vllm-scripts/run-vllm.sh`. My earlier claim that the repo edit prevented
regression was wrong; the marlin fix held only because `.env` overrode the live script's
stale default. Both copies reconciled (the live one had drifted by a `--load-format
bitsandbytes` line). Same trap as `deploy.sh`.

## Outcome

- ✅ Impact: Root-caused and **fixed** the flagship latency complaint — 4.7x faster answers from a one-flag change. Disproved the "1/4 GPU" premise that five customer-facing limitations were attributed to.
- 🧪 Tests: 5 live benchmarks before + 3 after on the production VM (decode, concurrency, prefill, bandwidth, /metrics, production-shaped e2e). No unit tests touched — no Rust changed.
- 📁 Files: `deploy/vllm/run-vllm.sh` (default -> awq_marlin + a warning comment so it cannot regress), `CHANGELOG_DEV.md` (2026-08-12 entry). VM: `.env` line 21.
- 🔁 Next prompts: retract the "1/4 GPU" claim from `noviq_limitations.docx` (both variants), ADR-0020 and the Kimi K3 note; re-measure under real concurrent load; then evaluate n-gram speculative decoding.
- 🧠 Reflection: The decisive move was refusing to reason from config. Two plausible hypotheses (long prompts inflating decode; GPU time-slicing) both *felt* right and were both **killed by measurement** — the flat 13.7 tok/s across a 300x context change, and the 8x concurrency scale-up. Reading `/metrics` instead of wall-clock split prefill from decode in one step and made the rest mechanical.

## Evaluation notes (flywheel)

- Failure modes observed: **A long-standing internal narrative went unchallenged for months.** "Time-sliced 1/4 GPU" was repeated across the limitations docs, ADR-0020, the Kimi K3 analysis and the GPU credits analysis — and was never tested against a bandwidth or FLOPS measurement. It also had a *plausible* mechanism, which is what kept it alive. Separately, the `forcing awq` warning has been in every vLLM startup log since 28 June and was read past each time.
- Graders run and results (PASS/FAIL): n/a — diagnostic session.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): flip `NOVIQ_VLLM_QUANT` to `awq_marlin`, restart `noviq-vllm`, confirm the `forcing awq` line is absent, and re-read `time_per_output_token_seconds` from `/metrics`. Single variable, ~2 min downtime, fully reversible.
