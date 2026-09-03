# Quickstart / Operator Runbook: vLLM Serving (016)

Target host: `<user>@<PRIVATE_IP>` (A40-12Q ~12 GB, driver 470). All inference stays
loopback; the public boundary remains `noviq-server` behind the Cloudflare tunnel.

> **Spike 0 first.** Do not build anything until the CUDA go/no-go passes (§1). If it
> fails, stop and open the IBA-IT driver-upgrade path (§7).

## 1. Spike 0 — CUDA forward-compat go/no-go (BLOCKER)

```bash
# A40 is datacenter-class + R470 ≥ R450 → CUDA 12 forward-compat is in scope.
# Install the userspace compat driver WITHOUT changing the kernel driver:
sudo apt-get install -y cuda-compat-12-4        # match the chosen vLLM's CUDA (e.g. 12.4)
export LD_LIBRARY_PATH=/usr/local/cuda/compat:$LD_LIBRARY_PATH

python3 -m venv ~/.vllm-venv && source ~/.vllm-venv/bin/activate
pip install "vllm>=0.8.5"                        # CUDA-12.x wheel; first line w/ stable Qwen3
export HF_HUB_OFFLINE=0                           # one-time download, then set =1
nvidia-smi                                        # confirm A40 visible
python -c "import torch; print(torch.cuda.is_available(), torch.version.cuda)"
#   EXPECT: True 12.x  → forward-compat works → PROCEED
#   If False / CUDA error → forward-compat blocked on this vGPU → STOP, go to §7
vllm serve Qwen/Qwen3-8B-AWQ --host 127.0.0.1 --port 8000 --max-model-len 4096 &
curl -s 127.0.0.1:8000/v1/completions -H 'content-type: application/json' \
  -d '{"model":"Qwen/Qwen3-8B-AWQ","prompt":"Say OK.","max_tokens":5}'
#   200 + a completion → SPIKE PASS.
```

## 2. Provision the sidecar (one-time, out-of-tree — survives git reset)

```bash
cd /opt/noviq/echoAI/deploy/vllm
./setup-vllm.sh        # builds ~/.vllm-venv, installs cuda-compat + vllm, downloads
                       # Qwen/Qwen3-8B-AWQ AND the GPTQ build into /opt/noviq/model-library
sudo cp noviq-vllm.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now noviq-vllm
curl -s 127.0.0.1:8000/health        # expect 200 once model loaded (cold start 10–30s)
```

`noviq-vllm.service` (sketch): `ExecStart=run-vllm.sh`, `Environment=LD_LIBRARY_PATH=…compat…`
`HF_HUB_OFFLINE=1`, `User=appadmin`, `Restart=on-failure`, `WantedBy=multi-user.target`.

## 3. VRAM tuning (fit ≤ 12 GB at 10 users)

Start: `--gpu-memory-utilization 0.85 --kv-cache-dtype fp8 --max-model-len 8192
--max-num-seqs 16`. Watch `nvidia-smi` under load; if it approaches the 12 GB ceiling,
lower `--max-model-len` (→ 6144/4096) or `--max-num-seqs`. FP8 KV ≈ 72 KB/token halves the
pool. (See research §R4.)

## 4. A/B + no-regression gate (before cutover)

```bash
cd /opt/noviq/echoAI/tools/vllm_eval
./run_ab.sh   # same legal Qs through: llama.cpp GGUF (baseline) | vLLM AWQ | vLLM GPTQ
              # scores grounding/citation correctness + latency
# PASS = AWQ or GPTQ >= baseline quality. Pick the winner → set NOVIQ_VLLM_MODEL.
```

## 5. Cutover (flip the engine)

```bash
# In /opt/noviq/echoAI/.env:
NOVIQ_INFER_ENGINE=vllm
NOVIQ_VLLM_URL=http://127.0.0.1:8000
NOVIQ_VLLM_MODEL=Qwen/Qwen3-8B-AWQ      # or the GPTQ winner
NOVIQ_VLLM_MAX_MODEL_LEN=8192
echo "P@ssw0rd" | sudo -S systemctl restart noviq-server
curl -s 127.0.0.1:8080/api/health        # expect engine.ready=true
```

## 6. Load test (acceptance SC-001..004)

```bash
cd tools/vllm_eval && ./load_test.sh 10   # then 20, 50
# records p50/p95, throughput, gpu util/mem, queue depth, failure rate
# PASS: 10 users 100% served (SC-001), p95 ≤ 10s (SC-002), ≥2.5× throughput (SC-003),
#       0 OOM over a multi-minute soak (SC-004).
```

## 7. Rollback / fallback

- **Instant rollback** (vLLM misbehaves): set `NOVIQ_INFER_ENGINE=llamacpp` in `.env`,
  restart `noviq-server`. Back to the known-good in-process Qwen3 path (FR-017). vLLM
  sidecar can keep running idle or be stopped.
- **Spike 0 failed / forward-compat blocked:** request IBA-IT to upgrade the **host vGPU
  manager + guest driver to R535** (CUDA 12.x), then retry §1. Until then, stay on the
  llama.cpp engine (current production).

## 8. Health & observability

- `noviq-server` `/api/health` gains `engine: { ready, num_running, num_waiting }`.
- vLLM `/metrics` → `vllm:num_requests_running|waiting` for queue depth.
- `query_log.latency_ms` continues per-turn; optionally label rows with the engine.
