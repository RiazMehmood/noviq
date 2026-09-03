# Phase 1 Data Model: vLLM Concurrent Inference Serving (016)

**No database schema change is required.** This feature swaps the generation engine; it
introduces **configuration** and **runtime** entities, not persisted relational data. The
PostgreSQL/pgvector corpus and all retrieval tables are untouched (Assumptions, spec).

## Configuration entities (env-driven, `.env`)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `NOVIQ_INFER_ENGINE` | enum `llamacpp` \| `vllm` | `llamacpp` | Selects the `TextGenerator` impl at server startup (FR-017 fallback switch). Flipped to `vllm` at cutover. |
| `NOVIQ_VLLM_URL` | url | `http://127.0.0.1:8000` | Loopback base URL of the vLLM sidecar (FR-007 — loopback only). |
| `NOVIQ_VLLM_MODEL` | string | `Qwen/Qwen3-8B-AWQ` | Model id/path the sidecar serves; A/B winner (AWQ or GPTQ). |
| `NOVIQ_VLLM_MAX_MODEL_LEN` | int | `8192` | Context ceiling (FR-010; ≥ 6k floor). Tuned by load test. |
| `NOVIQ_VLLM_TIMEOUT_MS` | int | `60000` | Client timeout before falling back / erroring gracefully (FR-011). |
| `NOVIQ_NO_THINK` | bool | `1` | Existing no-think flag — preserved; applied in the raw prompt the Rust side sends. |

vLLM-side launch flags (in `run-vllm.sh`, not Rust env): `--kv-cache-dtype fp8`,
`--gpu-memory-utilization 0.85`, `--max-num-seqs 16`, `--max-model-len 8192`,
`--served-model-name`, `--host 127.0.0.1 --port 8000`. Exact numbers finalized in the load
test (R4/R7).

## Runtime entities (in-memory / process)

- **Inference Engine (sidecar)** — the `noviq-vllm` systemd service. Attributes: `ready`
  (bool), `model_id`, `max_model_len`, `gpu_mem_budget`, `num_running`, `num_waiting`
  (queue depth). Exposed via vLLM `/health` + `/metrics`; surfaced to operators through
  `noviq-server` `/api/health.engine` (FR-013).

- **TextGenerator** (Rust trait) — abstraction over generation. Variants:
  `LlamaCppGenerator` (in-process `ModelSlot`, fallback) and `VllmGenerator` (HTTP client).
  Selected once at startup from `NOVIQ_INFER_ENGINE`.

- **Generation Request** — one chat turn. Fields: `prompt` (the exact hand-built ChatML
  string incl. no-think prefix), `max_tokens`, `temperature`/`top_p` (carried from current
  defaults), `stream` (true). Maps 1:1 to a user turn; no persistence beyond the existing
  `chat_message` + `query_log` rows.

- **Concurrency/Queue State** — `num_running` vs `num_waiting` from the sidecar; drives the
  capacity-exceeded edge case (admit-to-queue vs friendly retry) and the load-test metrics.

## Optional (only if useful, additive)

- `query_log.engine` (text, nullable) — label each query row with `vllm`|`llamacpp` for
  A/B and post-cutover monitoring. **Additive, nullable**; add only if the load-test/A/B
  analysis needs it in-DB rather than from logs. (Honors "additive migrations only".)

## State / lifecycle

```
boot → noviq-vllm starts (systemd, After=network) → loads model (cold start ~10–30s)
     → /health ready
noviq-server starts (Wants/After=noviq-vllm) → picks engine from env
request → query_router builds prompt (unchanged) → TextGenerator.generate_stream()
        → [vllm] POST /v1/completions stream=true → SSE deltas → chat:// events
        → [vllm down/timeout] → FR-011 graceful message (or fallback if configured)
```
