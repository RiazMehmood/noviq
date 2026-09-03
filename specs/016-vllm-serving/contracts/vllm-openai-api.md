# Contract: vLLM loopback OpenAI-compatible API (the dependency)

The `noviq-vllm` sidecar exposes vLLM's standard OpenAI-compatible server on **loopback
only** (`127.0.0.1:8000`, no public bind — FR-007). `noviq-server` is the only client.

## Endpoints used

### `POST /v1/completions` (primary — raw prompt, streaming)

Chosen over `/v1/chat/completions` so our **exact hand-built ChatML prompt** (including the
no-think `<think></think>` prefix) is sent verbatim — no server-side chat-template
re-rendering. Request:

```json
{
  "model": "Qwen/Qwen3-8B-AWQ",
  "prompt": "<|im_start|>system\n...<|im_end|>\n...<|im_start|>assistant\n<think>\n\n</think>\n\n",
  "max_tokens": 768,
  "temperature": 0.0,
  "top_p": 1.0,
  "stop": ["<|im_end|>", "</s>"],
  "stream": true
}
```

Streaming response = `text/event-stream`, one `data: {…}` per delta:

```
data: {"choices":[{"text":"The Secretary","index":0,"finish_reason":null}]}
data: {"choices":[{"text":" may sanction","index":0,"finish_reason":null}]}
data: {"choices":[{"text":"","index":0,"finish_reason":"stop"}]}
data: [DONE]
```

Client maps each `choices[0].text` delta → `on_token` → `chat://` event; stops on
`finish_reason` or `[DONE]`.

### `GET /health` (readiness)

`200 OK` once the model is loaded and serving. Used by the systemd `ExecStartPost` probe
and by `noviq-server` `/api/health.engine.ready`.

### `GET /metrics` (load/queue — optional)

Prometheus text exposition; `vllm:num_requests_running` and
`vllm:num_requests_waiting` feed the queue-depth signal (FR-013) and load-test metrics.

## Launch contract (`run-vllm.sh`)

```bash
vllm serve "$NOVIQ_VLLM_MODEL" \
  --host 127.0.0.1 --port 8000 \
  --served-model-name "$NOVIQ_VLLM_MODEL" \
  --quantization awq \                 # or gptq_marlin for the GPTQ build
  --kv-cache-dtype fp8 \               # FP8 storage KV (Ampere = storage-mode)
  --max-model-len "${NOVIQ_VLLM_MAX_MODEL_LEN:-8192}" \
  --gpu-memory-utilization 0.85 \
  --max-num-seqs 16 \
  --disable-log-requests
```

(Under CUDA forward-compat, the unit's `Environment=` prepends the `cuda-compat` libs to
`LD_LIBRARY_PATH` — see quickstart.)

## Invariants

- **Loopback only** — never bind `0.0.0.0`; no auth needed because it is unreachable off-box
  (FR-007). The public auth boundary stays at `noviq-server`.
- **Offline** — `--model` resolves to a **local path / pre-downloaded HF cache**; no network
  fetch at serve time (FR-006). Set `HF_HUB_OFFLINE=1`.
- The contract surface we depend on is the **stable OpenAI `/v1/completions` schema** +
  `/health`; a vLLM version bump must preserve these.
