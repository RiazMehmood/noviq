# Phase 0 Research: vLLM Concurrent Inference Serving (016)

**Date**: 2026-06-27 · **Branch**: `016-vllm-serving`

All [NEEDS CLARIFICATION] from the spec were resolved by the user (AWQ+GPTQ eval; p95
≤10 s; keep llama.cpp fallback). This document resolves the **technical unknowns** that
gate the plan, in priority order. The headline finding: **the CUDA/driver situation is a
hard prerequisite that must be proven by a Spike before any code is written.**

---

## R1 — CUDA / driver compatibility (HIGHEST RISK — gates the feature)

**Decision**: Run a **CUDA-12.x prebuilt vLLM (≥ 0.8.5)** on the existing **R470
datacenter driver** using the **CUDA forward-compatibility package** (`cuda-compat-12-x`),
**without** changing the guest driver. Prove it in **Spike 0** before committing to the
build. If forward-compat fails on the vGPU guest, escalate to **IBA IT to upgrade the
host vGPU manager + guest driver to R535** (CUDA 12.x) as the fallback path.

**Why** (the binding constraints):
- **Qwen3 requires vLLM ≥ 0.8.4.** That version line **dropped CUDA 11.x**; it ships only
  CUDA-12.x wheels. ([vLLM #17578](https://github.com/vllm-project/vllm/issues/17578))
- The **last `cu118` wheel is vLLM 0.6.1.post1**, which **predates Qwen3** and cannot load
  it. So "use an old vLLM on CUDA 11.4" is a **dead end** — there is no version that is
  simultaneously new enough for Qwen3 and old enough for CUDA 11.x.
- The host driver is **470** (CUDA toolkit ≤ 11.4 natively). Native install of a CUDA-12
  vLLM would refuse to run.
- **Forward compatibility** is NVIDIA's documented path for datacenters that cannot change
  drivers: the `cuda-compat` package ships a **userspace** CUDA driver that runs on the
  **older kernel module**. It requires a **datacenter GPU** and **driver ≥ R450**. The
  A40 is datacenter-class and R470 ≥ R450, so CUDA-12.x forward-compat is *in scope*.
  ([CUDA Forward Compatibility](https://docs.nvidia.com/deploy/cuda-compatibility/forward-compatibility.html))

**Residual risk**: forward-compat inside a **vGPU guest** (A40-**12Q**) is less commonly
exercised than on bare-metal datacenter cards; it may be constrained by the host vGPU
manager. This is exactly why **Spike 0** exists — it is a go/no-go gate, not an assumption.

**Alternatives considered**:
- *Build vLLM ≥ 0.8.5 from source against CUDA 11.8* — rejected: newer vLLM uses CUDA-12
  features/kernels; source builds against 11.8 routinely fail and are unmaintainable.
- *Guest driver upgrade first* — rejected as the **default** because a vGPU guest driver is
  gated by the **host** vGPU software (IBA-controlled); not self-serve. Kept as the
  **fallback** if forward-compat fails.
- *Containerized vLLM (NGC)* — does not bypass the driver floor; the container's CUDA still
  needs the host driver to satisfy it (via the same forward-compat mechanism). Container is
  a *packaging* choice, evaluated under R3, not a CUDA-compat escape hatch.

---

## R2 — vLLM version & model format

**Decision**: Target a **vLLM 0.8.5–0.9.x** CUDA-12.x wheel (first lines with stable Qwen3
support). Serve **Qwen3-8B**, evaluating **both**:
- **AWQ 4-bit** — `Qwen/Qwen3-8B-AWQ` (official Qwen build).
  ([HF](https://huggingface.co/Qwen/Qwen3-8B-AWQ))
- **GPTQ / W4A16** — `RedHatAI/Qwen3-8B-quantized.w4a16` (or `AlphaGaO/Qwen3-8B-GPTQ`).
  ([HF](https://huggingface.co/RedHatAI/Qwen3-8B-quantized.w4a16))

Both are **directly loadable by vLLM** (`vllm serve <model>`), downloaded once for offline
provisioning. **No retrain.** The A/B (R5) picks the launch winner; the loser is dropped.

**Rationale**: official + reputable pre-quantized builds exist for both formats, removing
the need to quantize ourselves. Weight footprint ~5–5.5 GB at 4-bit — fits the budget (R4).

---

## R3 — Process architecture (sidecar vs in-process)

**Decision**: vLLM runs as a **separate `noviq-vllm` systemd service (sidecar)** bound to
**loopback only** (`127.0.0.1`), exposing its **OpenAI-compatible HTTP API**
(`/v1/chat/completions`, streaming via SSE). `noviq-server` calls it over HTTP. The Python
vLLM runtime lives in a dedicated **venv** (or NGC container) provisioned **out-of-tree**
like the F006 ingestion sidecars / OCR venv, so it **survives `git reset --hard`** and is
**not** rebuilt by the Rust/FE poll-deploy.

**Why sidecar (not managed by noviq-server)**:
- Clean process isolation: a vLLM crash/OOM does not take down the Rust API; systemd
  restarts it; the Rust side degrades to fallback (R6/FR-011).
- vLLM is a long-lived, GPU-resident server — its natural deployment is its own service
  with its own readiness, exactly like the bundled PG sidecar from F011.
- The existing `noviq-deploy` poll-timer only rebuilds `src/`→FE and `src-tauri/`→cargo.
  The vLLM runtime + model are **provisioned once** (a setup script + `.env` knobs), not
  on every deploy — mirrors the Task-1 sidecar pattern. The deploy script gains a small
  "ensure vllm service active" check, not a rebuild step.

**Readiness/health**: `noviq-vllm` is `Type=...` with an `ExecStartPost`/health probe on
vLLM's `/health`; `noviq-server`'s `/api/health` adds an `engine: {ready, queue_depth}`
field by probing the sidecar (FR-013). Auto-start on boot via `WantedBy=multi-user.target`
+ `noviq-server` `After=/Wants=noviq-vllm` (FR-012).

**Alternatives**: in-process via PyO3 / launching vLLM as a child of noviq-server — rejected
(couples lifecycles, a Python GIL/asyncio runtime inside the Rust process, harder crash
isolation).

---

## R4 — VRAM budget on the 12 GB A40-12Q (must fit ≤ 12 GB at 10 users)

**Decision**: `gpu_memory_utilization ≈ 0.85` (≈10.2 GB ceiling, leaving headroom on the
12 GB slice), `kv_cache_dtype=fp8` (E5M2 storage), `max_model_len = 8192` (context cap,
≥ the FR-010 6k floor), `max_num_seqs ≈ 16` (admits the 10-user target with burst margin).
**Confirm empirically in Spike 0/load test** — these are starting points, not guarantees.

**Budget math** (Qwen3-8B, 4-bit, FP8 KV storage):
```
  ~5.3 GB   weights (4-bit AWQ/GPTQ)
  ~0.6 GB   CUDA context + activations + vLLM overhead
  ───────
  ~5.9 GB   fixed  → ~4–4.3 GB left for the paged KV pool under util=0.85
```
KV at **FP8** ≈ **72 KB/token** (half of FP16's ~144 KB/token for Qwen3-8B's GQA):
```
  10 users × 8,192 tokens × 72 KB  ≈ 5.9 GB   ← too big at full 8k for all 10
  10 users × 4,096 tokens × 72 KB  ≈ 2.9 GB   ← comfortable
```
**Reading**: 10 users at ~4k *effective* context fit easily; a full 8k for all 10
simultaneously is at the edge. Mitigation: PagedAttention only allocates KV for tokens
actually present (most legal turns are < 8k), and `max_model_len=8192` is a *ceiling* not a
per-request cost. The context-budget logic already caps retrieved context, so typical
turns sit well under 8k. **Net: fits, pending empirical confirmation.**

**FP8-on-Ampere note**: A40 is sm_86 (< 8.9), so FP8 is a **storage** format here — it
**halves KV memory** (the win we need) but does **not** add FP8 compute speedup; dequant is
done in-kernel. Confirmed acceptable. ([vLLM quantized KV cache](https://docs.vllm.ai/en/latest/features/quantization/quantized_kvcache/))

---

## R5 — AWQ vs GPTQ evaluation & no-regression gate

**Decision**: A small **offline A/B harness** runs the **same legal question set** through:
(a) current llama.cpp Qwen3-8B GGUF (baseline), (b) vLLM Qwen3-8B-AWQ, (c) vLLM
Qwen3-8B-GPTQ — scoring **grounding/citation correctness** (no new hallucinated/missing
citations vs baseline) and **latency**. Launch the format that **matches-or-beats baseline
quality** with the better latency. **Cutover is gated** on (b or c) ≥ baseline quality
(FR-014).

**Rationale**: government accuracy-first; 4-bit GPU quant can shift legal answer quality, so
the gate is mandatory, not optional. Reuse the existing eval question sets / grounding
validator rather than inventing a new judge.

**Question set**: reuse the curated legal prompts from prior eval rounds (e.g. the 20/22-prompt
constitution sets referenced in `CHANGELOG_DEV.md`); target ≥ 30 questions spanning lookup,
definition, cross-reference, and "missing evidence" (refusal) cases.

---

## R6 — Rust integration seam (the engine swap, fallback-safe)

**Decision**: Introduce a **`TextGenerator` trait** (async, streaming) with two impls:
- `LlamaCppGenerator` — wraps today's in-process `ModelSlot` path (the **fallback**).
- `VllmGenerator` — an HTTP client to the sidecar's OpenAI-compatible
  `/v1/chat/completions` with `stream=true`, mapping SSE deltas onto the existing
  `chat://` token events.

A single **env switch** (`NOVIQ_INFER_ENGINE = vllm | llamacpp`, default `llamacpp` until
cutover) selects the impl at startup (FR-017). **Everything upstream is unchanged**:
`query_router` builds the same prompt (incl. the no-think prefix), runs the same retrieval,
grounding, and citation gating; only the final "generate from this prompt, stream tokens"
call goes through the trait.

**Why a trait + HTTP (not FFI)**: keeps the Rust↔Python boundary at a stable HTTP contract
(OpenAI schema), preserves crash isolation, and makes the fallback a one-line config flip.
Smallest viable diff to the hot path (Constitution: smallest diff).

**Prompt format note**: vLLM's `/v1/chat/completions` applies the model's chat template
server-side. To keep our **hand-built ChatML + no-think** behavior identical, use vLLM's
**`/v1/completions`** (raw prompt, `prompt=<our exact string>`) **or** pass
`chat_template_kwargs={"enable_thinking": false}` — decided in Phase 1 contracts; default to
raw `/v1/completions` so the prompt is byte-identical to the llama.cpp path (no template
surprises, no-think preserved).

---

## R7 — Load testing

**Decision**: A repeatable load script drives **10 / 20 / 50** concurrent authenticated
`/api/query` requests against the VM, recording **p50/p95 end-to-end latency, throughput
(req/min), GPU util & mem (from `nvidia-smi` + `query_log`), queue depth, failure rate**.
Pass = SC-001 (100% of 10 served), SC-002 (p95 ≤ 10 s), SC-003 (≥ 2.5× throughput vs the
serialized baseline), SC-004 (0 OOM over a multi-minute soak). 20/50 characterize the
degradation curve and the admission/queue behavior (edge case: capacity exceeded).

**Tooling**: reuse the existing token-login + curl timing approach (already used for the
T028 / Qwen3 captures); a small concurrency wrapper (xargs -P / hey / a tiny Python asyncio
client) for the burst.

---

## Open items carried into Phase 1 / tasks

- **Spike 0 (BLOCKER)**: prove a CUDA-12.x vLLM serves Qwen3-8B-AWQ on the R470 vGPU via
  forward-compat. If it fails → IBA-IT driver-upgrade path before anything else.
- Exact `max_model_len` / `max_num_seqs` / `gpu_memory_utilization` from the load test.
- AWQ vs GPTQ winner from the A/B.
- `/v1/completions` raw-prompt vs `/v1/chat/completions`+template decision (lean raw).
