# vLLM Spike — Resume Runbook (copy-paste)

> **Why this file exists:** the previous launch failed with **SSH exit 255**, which is a
> *transport* failure — the Sangfor Easy Connect VPN drops a long-held SSH channel, and
> `launch.sh` runs `vllm serve` in the **foreground**, holding that channel open for the
> multi-minute model load. It is **NOT** a firewall / IBA IT-policy / auth / host-key block
> (verified: TCP 22 reachable, short SSH commands return exit 0, host key fine).
>
> **The fix:** SSH in *interactively* and start vllm **detached on the VM** with `setsid`
> (no install needed — `tmux` is not present on this box). The detached process keeps vllm
> alive even if your laptop's SSH/VPN drops. 255 can no longer kill it.
>
> The old tokenizer crash (`Qwen2Tokenizer has no attribute all_special_tokens_extended`)
> is **already fixed** — venv310 is now on `transformers 4.51.3 / tokenizers 0.21.4 /
> vllm 0.8.5`. The stale `serve.log` is from before the pin; the new run logs to `serve_run.log`.

---

## Step 1 — SSH into the VM (laptop terminal, VPN connected)

```bash
ssh <user>@<PRIVATE_IP>
```

## Step 2 — ONE-SHOT: fix deps + launch + verify (paste the whole block on the VM)

> Already done once, but kept idempotent: installs `setuptools`+`wheel` into the uv venv
> (Triton needs `setuptools` at runtime — its absence was the real "Engine core init failed"
> cause, NOT the VPN). Then launches detached with `setsid`, waits up to 200s, and prints
> either **HEALTH OK + a generation**, or the **root-cause error lines** if it crashed.

```bash
cd ~/vllm-spike

# 1) ensure runtime deps in the uv venv (idempotent)
VIRTUAL_ENV=$HOME/vllm-spike/venv310 ~/.local/bin/uv pip install -q setuptools wheel

# 1b) raise gpu-memory-utilization 0.30 -> 0.45 (0.30 left no room for KV cache;
#     0.45*12107=~5448MiB fits inside the ~5830MiB free alongside prod). Idempotent.
sed -i 's/--gpu-memory-utilization 0.30/--gpu-memory-utilization 0.45/' launch.sh
grep gpu-memory-utilization launch.sh

# 2) relaunch detached (survives SSH/VPN drop — no tmux needed)
pkill -f "vllm serve" 2>/dev/null; sleep 1
setsid bash launch.sh > serve_run.log 2>&1 < /dev/null &
disown
echo "launched; waiting for engine startup (up to ~200s)..."

# 3) poll: success -> smoke test ; crash -> print root cause
for i in $(seq 1 100); do
  if curl -sf 127.0.0.1:8009/health >/dev/null 2>&1; then
    echo "=== HEALTH OK ==="
    curl -s 127.0.0.1:8009/v1/completions \
      -H 'Content-Type: application/json' \
      -d '{"model":"Qwen/Qwen2.5-0.5B-Instruct","prompt":"Pakistan is a","max_tokens":16}'
    echo; break
  fi
  if ! pgrep -f "vllm serve" >/dev/null 2>&1; then
    echo "=== vllm DIED — ROOT CAUSE ==="
    grep -nE "ModuleNotFoundError|ImportError|RuntimeError|ValueError|AssertionError|No module named|out of memory|CUDA error|cannot|Failed to" serve_run.log \
      | grep -viE "Engine core initialization failed|HTTP Request" | tail -n 20
    echo "--- last 25 log lines ---"; tail -n 25 serve_run.log
    break
  fi
  sleep 2
done
```
OUTPUT:

=== vllm DIED — ROOT CAUSE ===
6:WARNING 06-28 10:58:53 [cuda.py:93] To see benefits of async output processing, enable CUDA graph. Since, enforce-eager is enabled, async output processor cannot be used
37:ERROR 06-28 10:59:02 [core.py:396]     raise ValueError("No available memory for the cache blocks. "
38:ERROR 06-28 10:59:02 [core.py:396] ValueError: No available memory for the cache blocks. Try increasing `gpu_memory_utilization` when initializing the engine.
60:    raise ValueError("No available memory for the cache blocks. "
61:ValueError: No available memory for the cache blocks. Try increasing `gpu_memory_utilization` when initializing the engine.
--- last 25 log lines ---
    return loop.run_until_complete(wrapper())
  File "uvloop/loop.pyx", line 1518, in uvloop.loop.Loop.run_until_complete
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/uvloop/__init__.py", line 48, in wrapper
    return await main
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/entrypoints/openai/api_server.py", line 1078, in run_server
    async with build_async_engine_client(args) as engine_client:
  File "/home/appadmin/.local/share/uv/python/cpython-3.10-linux-x86_64-gnu/lib/python3.10/contextlib.py", line 199, in __aenter__
    return await anext(self.gen)
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/entrypoints/openai/api_server.py", line 146, in build_async_engine_client
    async with build_async_engine_client_from_engine_args(
  File "/home/appadmin/.local/share/uv/python/cpython-3.10-linux-x86_64-gnu/lib/python3.10/contextlib.py", line 199, in __aenter__
    return await anext(self.gen)
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/entrypoints/openai/api_server.py", line 178, in build_async_engine_client_from_engine_args
    async_llm = AsyncLLM.from_vllm_config(
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/v1/engine/async_llm.py", line 150, in from_vllm_config
    return cls(
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/v1/engine/async_llm.py", line 118, in __init__
    self.engine_core = core_client_class(
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/v1/engine/core_client.py", line 642, in __init__
    super().__init__(
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/v1/engine/core_client.py", line 398, in __init__
    self._wait_for_engine_startup()
  File "/home/appadmin/vllm-spike/venv310/lib/python3.10/site-packages/vllm/v1/engine/core_client.py", line 430, in _wait_for_engine_startup
    raise RuntimeError("Engine core initialization failed. "
RuntimeError: Engine core initialization failed. See root cause above.





**If it printed HEALTH OK + a JSON completion → Spike 0 serve smoke is DONE.** ✅
Copy that whole output back here.

**If it printed ROOT CAUSE → copy those lines back here** and I'll give the next fix.

## Manual checks (optional)

```bash
tail -f ~/vllm-spike/serve_run.log    # live log (Ctrl-C to stop watching; server keeps running)
pgrep -af "vllm serve"                # is it alive?
```

## Stop / cleanup when done

```bash
pkill -f "vllm serve"            # stops the server
sleep 2; pgrep -af "vllm serve"  # confirm nothing left (should be blank)
nvidia-smi --query-gpu=memory.used,memory.total --format=csv,noheader
```

---

## Notes

- **VRAM (this smoke):** GPU was 6277/12107 MiB used (~5.8 GB free). `--gpu-memory-utilization 0.30`
  (~3.6 GB budget) was **too low** → "No available memory for the cache blocks". Raised to **0.45**
  (~5.4 GB budget, still inside the free ~5.8 GB). **Zero production impact.** Note: `util` is a
  fraction of **total** (12107 MiB), not of free — keep `util*12107` below current free VRAM.
- **Real 8B-AWQ fit test** is a *separate* step — needs a maintenance window:
  `sudo systemctl stop noviq-server` first to free VRAM, then serve `Qwen/Qwen3-8B-AWQ`,
  then restart noviq-server when done.
- **tmux is NOT installed** on this box — that's why we use `setsid`. If you want it for nicer
  reattach: `sudo apt-get install -y tmux` (sudo pw `P@ssw0rd`), optional.
- **Reconnect later & check it's alive:** `pgrep -af "vllm serve"` then `tail -f ~/vllm-spike/serve_run.log`.
- **launch.sh** (already on VM at `~/vllm-spike/launch.sh`):
  ```bash
  export LD_LIBRARY_PATH=$HOME/vllm-spike/compat-extract/usr/local/cuda-12.4/compat:$LD_LIBRARY_PATH
  exec venv310/bin/vllm serve Qwen/Qwen2.5-0.5B-Instruct \
    --host 127.0.0.1 --port 8009 \
    --gpu-memory-utilization 0.45 --max-model-len 2048 --enforce-eager   # was 0.30 (too low for KV)
  ```
