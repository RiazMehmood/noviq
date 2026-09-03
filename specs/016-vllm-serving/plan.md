# Implementation Plan: vLLM Concurrent Inference Serving

**Branch**: `016-vllm-serving` | **Date**: 2026-06-27 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/016-vllm-serving/spec.md`

## Summary

Replace the server's single, serialized in-process llama.cpp generation (one `ModelSlot`,
`gen_gate = Semaphore::new(1)`) with a **vLLM continuous-batching sidecar** so the headless
server meets the **≥ 10 concurrent users** hard requirement on the **12 GB A40-12Q**.
vLLM serves **Qwen3-8B** in a 4-bit GPU quantization (AWQ vs GPTQ chosen by A/B) with **FP8
KV cache** (storage-mode on Ampere). The Rust server calls vLLM over its **loopback
OpenAI-compatible HTTP API** behind a new `TextGenerator` trait; the **existing llama.cpp
path is retained as a config-switchable fallback**. Retrieval, prompt building (incl.
no-think), grounding, and citation gating are **unchanged**. **The single largest risk —
CUDA 12.x on the R470 vGPU driver — is proven by Spike 0 before any production code.**

## Technical Context

**Language/Version**: Rust 1.87 (`src-tauri`, `noviq-server` bin); Python 3.10+ for the
vLLM sidecar (out-of-tree venv/container, not in the cargo workspace).
**Primary Dependencies**: vLLM ≥ 0.8.5 (CUDA-12.x wheel) + CUDA forward-compat
(`cuda-compat-12-x`); existing axum 0.7, sqlx 0.8 (Postgres), llama-cpp-2 0.1.143 (retained
for fallback); a Rust HTTP client (`reqwest`, already in tree) + SSE parsing for streaming.
**Storage**: bundled PostgreSQL + pgvector (unchanged); model weights on disk under
`/opt/noviq/model-library/` (AWQ/GPTQ Qwen3-8B); no schema change required (engine selection
is env-config; load-test/eval results recorded as docs, optional additive `query_log`
columns for engine label only if useful).
**Testing**: `cargo test` (trait + HTTP client unit tests, SSE mapping); offline A/B eval
harness (R5); load-test script 10/20/50 (R7); Spike 0 manual go/no-go.
**Target Platform**: Ubuntu 20.04 GPU VM (`<user>@<PRIVATE_IP>`), A40-12Q ~12 GB vGPU,
driver 470. Server-only; desktop Tauri app untouched.
**Project Type**: web (Rust backend `src-tauri` + SolidJS frontend `src`), server deployment.
**Performance Goals**: p95 ≤ 10 s and p50 ~5 s at 10 concurrent users; ≥ 2.5× throughput vs
serialized baseline; 0 OOM over a multi-minute 10-user soak.
**Constraints**: fully offline; ≤ 12 GB VRAM; inference engine loopback-only (no public
port); smallest viable diff to the hot path; keep fallback.
**Scale/Scope**: launch = 10 concurrent users, single GPU; design must not preclude later
multi-replica/multi-GPU scale-out (FR-015).

## Constitution Check

*GATE: evaluated against all 13 principles (I–VIII Core, IX–XIII Domain).*

| Principle | Verdict | Notes |
|---|---|---|
| I. Offline-Always (NON-NEG) | ✅ PASS | vLLM + weights are local; loopback-only; zero external calls (SC-007). |
| II. Tiered Intelligence | ✅ PASS | Generation tier unchanged in spirit; vLLM is the worker engine for the server. |
| **III. Single Model Slot / GGUF Q4_K_M (NON-NEG)** | ⚠️ **DIVERGES** | vLLM holds the model **continuously resident** in VRAM (not demand-load/evict) and uses **AWQ/GPTQ** (not GGUF). **Server-only**; desktop keeps the single-slot GGUF rule. See Complexity Tracking + ADR. |
| IV. SQLite single source of truth | ✅ N/A (already amended for server) | Server runs on Postgres (F011/F012 divergence); this feature adds no DB. |
| V. Pending Actions Queue | ✅ N/A | No dangerous tool calls introduced. |
| VI. Persona-as-Config | ✅ PASS | No agent/persona change. |
| VII. Hardware-Adaptive | ✅ PASS | Server is a fixed known host; engine selection is explicit env config; graceful degrade to fallback (FR-011). |
| VIII. Incremental delivery | ✅ PASS | P1 (10-user serving) is an independently shippable MVP. |
| IX. Pakistan Legal Domain (NON-NEG) | ✅ PASS | Same model, same corpus, same domain. |
| X. Officer-First UX | ✅ PASS | Improves officer experience (no queueing); UX otherwise identical. |
| XI. Citation Discipline (NON-NEG) | ✅ PASS | Citation/grounding pipeline unchanged; no-regression gate (FR-014). |
| XII. Latest Text + Amendment Chain | ✅ PASS | Unaffected (retrieval unchanged). |
| XIII. Cross-Reference Graph | ✅ PASS | Unaffected. |
| **Tech-Stack Invariant** (llama-cpp-2 / GGUF Q4_K_M) | ⚠️ **DIVERGES** | Adds vLLM as a **server-side** inference engine + AWQ/GPTQ format. "Deviation requires a constitution amendment with ADR." See Complexity Tracking. |
| Dev constraint: p95 < 10 s (Tier 3/4) | ✅ PASS | Target p95 ≤ 10 s aligns. |
| Dev constraint: smallest viable diff | ✅ PASS | Hot path changed only at the generation call behind a trait; fallback preserved. |

**Gate result**: PASS **with two declared divergences** (Principle III + Tech-Stack
Invariant), both **server-only** and consistent with the existing precedent that the
headless server diverges from desktop-oriented invariants (Postgres vs SQLite, F011/F012).
Requires an **ADR** and, on cutover, a **constitution amendment** recording the server-side
inference-engine exception. Justified in Complexity Tracking below.

## Project Structure

### Documentation (this feature)

```text
specs/016-vllm-serving/
├── plan.md              # This file
├── research.md          # Phase 0 — CUDA/vLLM/format/VRAM/seam/load-test decisions
├── data-model.md        # Phase 1 — config + runtime entities (no DB schema change)
├── quickstart.md        # Phase 1 — operator runbook (provision, deploy, cutover, rollback)
├── contracts/
│   ├── text-generator-trait.md     # internal Rust generation seam
│   └── vllm-openai-api.md          # the loopback HTTP contract we depend on
└── tasks.md             # Phase 2 (/sp.tasks — NOT created here)
```

### Source Code (repository root)

```text
src-tauri/
├── src/
│   ├── bin/noviq-server.rs              # remove/raise gen_gate when engine=vllm; wire engine select
│   ├── services/
│   │   ├── llm.rs                       # existing in-process path → LlamaCppGenerator (fallback)
│   │   ├── inference/                   # NEW module
│   │   │   ├── mod.rs                   #   TextGenerator trait + engine selection (env)
│   │   │   ├── llamacpp.rs              #   wraps current ModelSlot (fallback impl)
│   │   │   └── vllm.rs                  #   HTTP client to vLLM, SSE→chat:// streaming
│   │   └── query_router.rs             # call site swaps ModelSlot→TextGenerator (only change)
│   └── api/health.rs                   # add engine readiness/queue to /api/health (FR-013)
│
deploy/
├── vllm/
│   ├── setup-vllm.sh                   # one-time: venv/container + cuda-compat + download AWQ/GPTQ
│   ├── noviq-vllm.service              # systemd sidecar unit (loopback OpenAI API)
│   └── run-vllm.sh                     # launch w/ tuned flags (max-model-len, kv fp8, util)
└── deploy.sh                           # +"ensure noviq-vllm active" check (no rebuild of vllm)

tools/
└── vllm_eval/                          # R5 A/B harness (llama.cpp vs AWQ vs GPTQ) + R7 load test
```

**Structure Decision**: Web/server layout. The **Rust seam** lives in a new
`src-tauri/src/services/inference/` module (trait + two impls) so the call site in
`query_router` changes minimally. The **vLLM runtime is out-of-tree** under `deploy/vllm/`
+ provisioned on the VM (venv/container under `/opt/noviq`, like the F006 sidecars), so the
poll-deploy never rebuilds it. Eval/load tooling under `tools/vllm_eval/`.

## Phasing (delivery order — Spike gates everything)

- **Spike 0 — CUDA go/no-go (BLOCKER, no production code):** on the VM, install
  `cuda-compat-12-x`, `pip install vllm>=0.8.5` (cu12), `vllm serve Qwen/Qwen3-8B-AWQ`
  loopback; confirm it loads on the R470 vGPU and answers one request. **If it fails →
  stop; open the IBA-IT driver-upgrade path; do not proceed.**
- **Phase 1 — Sidecar provisioning (P3 infra):** `setup-vllm.sh`, `noviq-vllm.service`,
  loopback API up, auto-start, `/api/health` engine field. Tune VRAM flags (R4).
- **Phase 2 — Rust seam (P1 core):** `TextGenerator` trait + `VllmGenerator` (raw
  `/v1/completions`, SSE→`chat://`) + `LlamaCppGenerator` fallback; env switch; lift
  `gen_gate` when engine=vllm. Quality parity check on a few prompts.
- **Phase 3 — A/B + no-regression gate (P1):** run the eval harness (R5); pick AWQ|GPTQ;
  confirm ≥ baseline grounding/citation quality. **Cutover gate.**
- **Phase 4 — Load test + tune (P1):** 10/20/50 concurrent (R7); confirm SC-001..004;
  finalize `max_num_seqs`/`max_model_len`.
- **Phase 5 — Cutover + docs:** flip `NOVIQ_INFER_ENGINE=vllm`; quickstart + changelog;
  ADR + constitution-amendment note. Keep fallback flip documented.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **Principle III — model continuously resident in VRAM (not demand-load/evict), AWQ/GPTQ not GGUF** | The 10-concurrent-user hard requirement is **physically unachievable** with a single serialized GGUF slot — llama.cpp throughput is flat under concurrency. vLLM's continuous batching + a resident model is the mechanism that serves 10 users on one 12 GB GPU. | Keeping the single GGUF slot (the simpler, constitution-default option) **cannot meet FR-002** at all; no amount of tuning makes a width-1 gate serve 10 users at p95 ≤ 10 s. Scope-limited to the **server**; desktop keeps the rule. |
| **Tech-Stack Invariant — add vLLM engine (+ AWQ/GPTQ) server-side** | Same as above; vLLM is the only mature offline engine providing continuous batching + PagedAttention within 12 GB. | A custom batching layer over llama.cpp would reinvent vLLM at far higher cost/risk and still lack PagedAttention. Containerized/managed alt-engines (TGI/SGLang) considered; vLLM chosen for maturity + Qwen3 + AWQ/GPTQ support. |

> Both divergences are **server-only**, mirror the existing **Postgres-vs-SQLite** server
> divergence (F011/F012), and are gated by an **ADR** + a **constitution amendment** at
> cutover. The desktop application remains fully compliant with Principle III and the
> Tech-Stack Invariant.

## Risks (carried from research.md)

1. **CUDA forward-compat on the vGPU guest may not work** (HIGH) → Spike 0 gate; fallback =
   IBA-IT host vGPU/driver upgrade to R535.
2. **12 GB headroom at full 8k × 10 users** (MEDIUM) → cap `max_model_len`, FP8 KV, empirical
   load test; PagedAttention only allocates KV for present tokens.
3. **4-bit quant quality shift on legal answers** (MEDIUM) → FR-014 A/B no-regression gate.
4. **Operational footprint of a second runtime** (LOW-MED) → systemd sidecar + auto-start +
   health, modeled on the bundled-PG sidecar.

## Next

`/sp.tasks` to generate the dependency-ordered task list (Spike 0 first as a hard gate),
then `/sp.adr` for the inference-engine divergence decision.
