# ADR-0018: Server-Side Inference Engine — vLLM Continuous-Batching Sidecar

> **Scope**: Document decision clusters, not individual technology choices. Group related decisions that work together (e.g., "Frontend Stack" not separate ADRs for framework, styling, deployment).

- **Status:** Accepted
- **Date:** 2026-06-28
- **Feature:** 016-vllm-serving
- **Context:** The headless server (F014) must serve **≥ 10 concurrent users** on a single **12 GB A40-12Q** vGPU (FR-002, SC-001..004: p95 ≤ 10 s, p50 ~5 s, ≥ 2.5× throughput, 0 OOM). The current generation path is a **single in-process llama.cpp `ModelSlot` serialized behind `gen_gate = Semaphore::new(1)`** — width-1, so concurrent turns queue and latency collapses. llama.cpp throughput is flat under concurrency; no tuning of a width-1 gate can meet a 10-user target. The constitution's **Principle III (single demand-loaded GGUF Q4_K_M slot)** and the **Tech-Stack Invariant (llama-cpp-2 / GGUF)** are written for the desktop app and are the blocker here. The single largest technical risk — running a CUDA-12.x engine on the R470 vGPU driver — was retired by **Spike 0** (forward-compat proven) and the **serve smoke** (vLLM 0.8.5 served Qwen2.5-0.5B on the vGPU, `/health` OK + real generation, alongside production, zero prod impact) on 2026-06-28 before any production code.

<!-- Significance checklist (ALL must be true to justify this ADR)
     1) Impact: Long-term consequence for architecture/platform/security?  → YES (inference engine, VRAM model, deployment topology)
     2) Alternatives: Multiple viable options considered with tradeoffs?    → YES (keep llama.cpp gate, custom batching, TGI/SGLang, in-process PyO3)
     3) Scope: Cross-cutting concern (not an isolated detail)?              → YES (hot generation path, systemd topology, constitution divergence)
-->

## Decision

Adopt **vLLM as the server-side inference engine**, integrated as one cluster:

- **Engine & batching**: vLLM ≥ 0.8.5 (CUDA-12.x wheel) running **continuous batching + PagedAttention** — the mechanism that serves 10 users on one 12 GB GPU. Runs on the R470 vGPU via the **CUDA forward-compat** package (`cuda-compat-12-x`, locally extracted, no host driver upgrade).
- **Process topology**: a **loopback-only `noviq-vllm` systemd sidecar** exposing vLLM's **OpenAI-compatible HTTP API** (`127.0.0.1`, no public port). The Python runtime lives in an **out-of-tree venv** under `/opt/noviq` (like the F006 ingestion/OCR sidecars and the F011 bundled-PG sidecar), so it **survives `git reset --hard`** and is **not** rebuilt by the poll-deploy timer (which gains only an "ensure vllm active" check).
- **Model & memory**: **Qwen3-8B in 4-bit GPU quantization (AWQ vs GPTQ, picked by A/B)**, held **continuously resident** in VRAM with **FP8 KV cache** (storage-mode on Ampere sm_86 — halves KV, no FP8 compute). Tuning starting points: `gpu_memory_utilization ≈ 0.85`, `max_model_len = 8192`, `max_num_seqs ≈ 16`, confirmed by load test. (Spike note: `gpu_memory_utilization` is a fraction of **total** VRAM and counts other processes' usage — it must be set **above** resident-prod usage, not below.)
- **Rust seam**: a new async streaming **`TextGenerator` trait** in `src-tauri/src/services/inference/` with two impls — **`VllmGenerator`** (raw `/v1/completions` so the hand-built ChatML + no-think prompt is byte-identical, SSE→existing `chat://` token events) and **`LlamaCppGenerator`** (wraps today's `ModelSlot`, retained as fallback). A single env switch **`NOVIQ_INFER_ENGINE = vllm | llamacpp`** (default `llamacpp` until cutover) selects the impl; `gen_gate` is lifted when `engine=vllm`. Retrieval, prompt building, grounding, and citation gating are **unchanged** (FR-014 no-regression gate gates cutover).

**Constitution divergence (server-only):** this diverges **Principle III** (resident, not demand-load/evict; AWQ/GPTQ not GGUF) and the **Tech-Stack Invariant** (adds vLLM). Both are scoped to the **server**; the desktop app remains fully compliant. Requires a **constitution amendment** at cutover, mirroring the existing **Postgres-vs-SQLite** server exception (ADR-0014/0015).

## Consequences

### Positive

- **Meets the 10-user hard requirement** — continuous batching + PagedAttention is the only offline mechanism that serves 10 concurrent users within 12 GB at p95 ≤ 10 s; the width-1 gate cannot at any tuning.
- **Crash isolation** — a vLLM OOM/crash does not take down the Rust API; systemd restarts the sidecar and the Rust side degrades to the llama.cpp fallback (FR-011).
- **Reversible cutover** — fallback is a one-line env flip (`NOVIQ_INFER_ENGINE=llamacpp`); default stays `llamacpp` until the A/B + load gates pass.
- **Smallest viable diff to the hot path** — only the final "generate + stream" call moves behind a trait; the entire retrieval/grounding/citation pipeline is untouched.
- **Deploy-safe provisioning** — out-of-tree venv + systemd means the poll-deploy never rebuilds the Python runtime or re-downloads weights.
- **Scale-out headroom** — a loopback HTTP contract does not preclude later multi-replica / multi-GPU scale-out (FR-015).

### Negative

- **Second runtime to operate** — a Python/CUDA GPU server alongside the Rust API: more provisioning, monitoring, and a CUDA forward-compat dependency pinned to this vGPU/driver pair (mitigated by sidecar + auto-start + `/api/health` engine field, modeled on bundled-PG).
- **Constitution divergence** — breaks two invariants for the server; requires an amendment and ongoing "desktop vs server" mental model.
- **Resident VRAM cost** — model is always loaded (no evict), permanently consuming VRAM on the shared 12 GB slice; full 8k context for all 10 users simultaneously is at the edge (mitigated by FP8 KV + PagedAttention + context-budget cap).
- **4-bit quant quality risk** — AWQ/GPTQ may shift legal answer quality vs the GGUF baseline; mandatory FR-014 A/B no-regression gate before cutover.
- **Rust↔Python HTTP boundary** — adds serialization + SSE parsing on the hot path (accepted: stable OpenAI contract, far simpler/safer than FFI/PyO3).
- **Forward-compat fragility** — a host vGPU-manager change by IBA IT could break `cuda-compat`; fallback path is an IBA-IT driver upgrade to R535.

## Alternatives Considered

- **Keep the single serialized llama.cpp GGUF slot (constitution default)** — *rejected*: physically cannot meet FR-002; a width-1 `gen_gate` serves ~3–4 users at acceptable latency, never 10. This is the status quo the feature exists to replace.
- **Custom batching layer over llama.cpp** — *rejected*: reinvents vLLM (continuous batching + PagedAttention) at far higher cost/risk and still lacks PagedAttention's KV paging; unmaintainable.
- **Other batching servers (TGI / SGLang)** — *rejected for launch*: viable architecturally but vLLM has the most mature offline Qwen3 + AWQ/GPTQ support and the OpenAI-compatible API we need; chosen over equivalents for ecosystem maturity. (Re-evaluable later — the `TextGenerator` trait keeps the engine swappable.)
- **In-process vLLM via PyO3 / vLLM as a child of `noviq-server`** — *rejected*: couples lifecycles, embeds a Python GIL/asyncio runtime in the Rust process, and destroys crash isolation.
- **Older `cu118` vLLM on the native CUDA-11.4 driver** — *rejected*: a dead end — the last `cu118` wheel (0.6.1.post1) predates Qwen3, and Qwen3 needs vLLM ≥ 0.8.4 which is CUDA-12 only. No version is simultaneously new enough for Qwen3 and old enough for CUDA 11.x.
- **Guest driver upgrade to R535 first** — *rejected as default* (kept as fallback): a vGPU guest driver is gated by the IBA-controlled host vGPU manager, not self-serve; Spike 0 proved forward-compat works without it.

## References

- Feature Spec: [specs/016-vllm-serving/spec.md](../../specs/016-vllm-serving/spec.md)
- Implementation Plan: [specs/016-vllm-serving/plan.md](../../specs/016-vllm-serving/plan.md)
- Research (R1–R7): [specs/016-vllm-serving/research.md](../../specs/016-vllm-serving/research.md)
- Contracts: [text-generator-trait.md](../../specs/016-vllm-serving/contracts/text-generator-trait.md), [vllm-openai-api.md](../../specs/016-vllm-serving/contracts/vllm-openai-api.md)
- Related ADRs: **ADR-0001** (LLM model hosting / single-slot isolation — this ADR supersedes it **for the server only**; desktop unchanged); **ADR-0014/0015** (PostgreSQL re-platform — precedent for a server-only constitution divergence); **ADR-0017** (server deployment & update strategy — sidecar/poll-deploy pattern reused)
- Evaluator Evidence: Spike 0 CUDA forward-compat PASS + serve smoke DONE (2026-06-28) — `specs/016-vllm-serving/VLLM_SPIKE_RESUME.md`; deployment memory `project_echoai-headless-deployment.md`
