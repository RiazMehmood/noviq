# Gemma 3 4B Staging Report — 2026-07-06

**Status: Gemma 3 4B (bitsandbytes 4-bit) is LIVE on prod** with the full v5.4.0
grounding prompt, after three failed quantization attempts and two
infrastructure bottlenecks found and fixed during staging.

---

## 1. Why 4B

| Model | State | Problem |
|---|---|---|
| Qwen3-8B-AWQ (vLLM) | previous prod | Good quality, ~6 s single query, but heavy on the 12 GB card |
| Gemma 3 1B bf16 | prod until today | ~2 s turns, but comprehension at its ceiling: invented labels, partial lists; needed the compact prompt (v5.5.0-compact) just to stop over-refusing |
| **Gemma 3 4B int4** | **target** | ~3 GB weights — smaller than the 8B-AWQ's ~5.7 GB, expected quality well above 1B |

The size argument (from the user's experts) was correct. What broke was the
**kernels**, not the arithmetic.

## 2. The environment that makes this hard

- **GPU**: NVIDIA A40-12Q vGPU, 12 GB, shared/time-sliced (IBA Karachi VM)
- **Driver**: 470.223.02 (2021-era, native CUDA ≤ 11.4)
- **CUDA-12 forward-compat shim** (`cuda-compat-12-4`, extracted locally, no
  driver upgrade) so vLLM 0.8.5's CUDA-12 wheels run at all
- **Arch**: sm_86 (Ampere)
- vLLM **0.8.5** (April 2025 — released right as Gemma 3 shipped; its Gemma 3
  multimodal support was brand new)

Almost nobody tests this stack combination. The text-only 1B ran fine on it;
the 4B did not, because…

## 3. The key architectural difference: 1B vs 4B

Gemma 3 **1B is text-only** (`Gemma3ForCausalLM`). Gemma 3 **4B/12B/27B are
multimodal** (`Gemma3ForConditionalGeneration` — a vision tower wrapped around
the language model). vLLM loads the 4B through a different, newer code path,
and quantized inference interacts with it differently.

## 4. What was tested and how each attempt failed

### Attempt 1 — AWQ int4 (`gaunernst/gemma-3-4b-it-qat-autoawq`, 3.8 GB) ❌
Loaded fine. Generated **empty text**; `logprobs=1` returned **NaN** ("Out of
range float values are not JSON compliant"). NaN logits = the forward pass
itself produces garbage numbers.

### Attempt 2 — GPTQ (`ISTA-DASLab/gemma-3-4b-it-GPTQ-4b-128g`) ❌
The repo is actually compressed-tensors format (`QUANT=gptq_marlin` errors
"does not match"; `QUANT=none` auto-detects it). Loaded, produced the **exact
same empty/NaN output**. Two independent int4 formats failing identically ⇒
the problem is the quantized-kernel execution path, not one bad checkpoint.

**Likely root cause**: vLLM's int4 dequant/matmul kernels (AWQ/Marlin) compute
in **float16**. Gemma 3 was trained in bfloat16 with large activation
magnitudes that are known to overflow float16 → inf → NaN propagates through
every layer. The 1B never hits this because it runs unquantized in bf16 and
never touches those fp16 kernels. Compounded by the immature
`Gemma3ForConditionalGeneration` support in 0.8.5 on our odd compat stack.

### Attempt 3 — Unquantized bf16 (`unsloth/gemma-3-4b-it`, 8.1 GB) ❌
Dodges the kernel problem entirely (same path as the working 1B) but the
weights alone are 8.1 GB. With ~1 GB used by other processes on the shared
12 GB card there was **no room for the KV cache** — "No available memory for
the cache blocks" crash-loop even at `util 0.92`, `MAX_LEN 6144`, `seqs 4`.

### Attempt 4 — bitsandbytes 4-bit (`unsloth/gemma-3-4b-it-bnb-4bit`) ✅ WORKS
bitsandbytes NF4 uses a **different dequantization path that computes in the
model's native dtype (bf16)**, avoiding the fp16 AWQ/Marlin kernels that
NaN'd. Result:

- Weights load at **3.57 GiB** (vs 8.1 GB bf16) — fits with a big KV pool
- Smoke test: real text, **finite logprobs** (all ≈ 0 for a factual answer),
  clean `<end_of_turn>` stop — exactly where AWQ/GPTQ died
- Setup quirks hit on the way (recorded so nobody re-trips them):
  - the vLLM venv was created with `uv` → **no `pip` binary**; use
    `python -m pip` (`ensurepip` was run earlier)
  - vLLM's bnb loader **lists the HF repo tree online even when cached**,
    which crashes under `HF_HUB_OFFLINE=1` (set in `run-vllm.sh`). Fix that
    keeps the offline invariant: point `NOVIQ_VLLM_MODEL` at the **local
    snapshot directory** instead of the repo id.

## 5. Current production configuration

| Key | Value |
|---|---|
| `NOVIQ_VLLM_MODEL` | `~/.cache/huggingface/hub/models--unsloth--gemma-3-4b-it-bnb-4bit/snapshots/eb03c885…` (local path — offline-safe) |
| `NOVIQ_VLLM_QUANT` | `bitsandbytes` (adds `--load-format bitsandbytes`) |
| `NOVIQ_VLLM_PROMPT_FORMAT` | `gemma` (ChatML→Gemma translation at the `VllmGenerator` seam) |
| `NOVIQ_PROMPT_PROFILE` | `full` → **v5.4.0** grounding prompt (compact was a 1B workaround; 4B doesn't need it) |
| `NOVIQ_VLLM_MAX_MODEL_LEN` | 8192 (the Rust side's prompt+gen budget) |
| `NOVIQ_VLLM_LEN_HEADROOM` | **6144** → vLLM serves an 14336-token window (see bottleneck A) |
| `NOVIQ_VLLM_GPU_UTIL` / `MAX_NUM_SEQS` | 0.80 / 16 |
| Revert to 1B | restore `MODEL=unsloth/gemma-3-1b-it`, `QUANT=none`, `PROMPT_PROFILE=compact` + restart both services |
| Revert to 8B | restore `.env.bak.qwen8b.20260706` + restart both services |

### Measured behaviour (staging, 2026-07-06)

| Query type | Result |
|---|---|
| Simple guidance ("direct contracting under SPPRA") | **5.8 s**, verbatim Rule 16(b) quote, cited |
| Guidance with 6 citations | 10.6 s, grounding score 0.86 |
| Map-reduce (ActionItems trigger) | ~4 min wall, grounding score **1.50, 15 citations, 0 uncited** — the best grounding of the session |
| Out-of-corpus ("capital of Sindh") | 1.0 s, **correct refusal** (no hallucination) |
| Throughput | prompt 650–1400 tok/s, generation **60–66 tok/s** single-stream |

For comparison: the 1B gave ~2 s answers but with invented labels and partial
lists (model ceiling); the 8B-AWQ gave ~6 s single-query answers.

## 6. Bottlenecks found in staging, and how they are / can be removed

### A. Token-budget mismatch → vLLM 400s (mitigated; proper fix pending)
The Rust server budgets prompt+completion to 8192 tokens using a **char-based
estimate**; vLLM enforces the window with the **real Gemma tokenizer**. On
dense legal text the real count ran ~1.6 chars/token — one map-pass prompt
estimated ≤ 8192 was actually **13,243 tokens** → HTTP 400 → the whole turn
errored. This is worst on retrieved chunks with citation markers, deep
numbering, and non-Latin fragments, so it varies per query.

- **Mitigation shipped**: `NOVIQ_VLLM_LEN_HEADROOM=6144` (serve window 14336).
  No 400s since.
- **Proper fix (open)**: clamp `max_tokens` against the real prompt count in
  `src-tauri/src/services/inference/vllm.rs` (already flagged as a follow-up
  in `run-vllm.sh` comments), or count prompt tokens with the served model's
  tokenizer. Headroom alone is whack-a-mole.

### B. Tunnel kills long requests → answers lost (FIXED — detach + poll)
The Cloudflare quick tunnel terminates HTTP requests at **~100 s**. Map-reduce
turns legitimately run minutes. Worse, axum **cancels the handler future on
client disconnect**, so the server aborted the turn mid-pipeline and persisted
nothing — the browser showed `error_apierror` and the work was thrown away.

**Fix shipped (branch `fix-tunnel-detach-poll`)**:
- `POST /api/query` now runs the turn in a **detached `tokio::spawn` task** —
  a dropped connection no longer cancels it; the answer always completes and
  persists to `query_log` (the gen-gate permit is held inside the task, so the
  llama.cpp serialization invariant survives disconnects too).
- New **`POST /api/poll_turn`** (auth'd, scoped to the caller's user id +
  conversation, `since_ms` guard so it can never return a stale turn).
- `ChatPage` on a failed send (web only) polls every 6 s for up to 12 min,
  shows "answer is still being prepared…", and renders the recovered answer
  with citations. A turn that failed server-side never writes a row, so real
  errors still surface.

### C. Map-reduce latency (~4 min) (open — options identified)
Triggered by cue words (`responsibilities`, `duties`, `summarize <named act>`,
`compare X with Y`, `conflict/contradiction`, …) → one grounded LLM pass per
window-sized batch, **sequential**, then a reduce pass. Options to shrink it:
1. Smaller map scope: cap in-scope provisions / batches for ActionItems when
   the query names an instrument (most "responsibilities of X under Y rules"
   questions don't need whole-corpus coverage).
2. 4B generates ~60 tok/s (vs ~13 tok/s for the GGUF 8B) — cutting map-pass
   `max_tokens` (extracts don't need 1024) is nearly free latency.
3. vLLM can batch concurrently — the sequential-passes rule exists for the
   single llama.cpp slot (Constitution III); under the vLLM engine the map
   passes could run batched. Bigger change; needs care with VRAM.

### D. Delivery of live tokens over the tunnel (open, deferred)
The tunnel buffers SSE (known from F021 — polling fallback shipped there).
Proper fix remains WebSockets. Detach+poll makes this less urgent: answers are
never lost, just not streamed live.

## 7. Does the new strategy reduce model performance?

**Detach + poll: no effect on model quality whatsoever.** It changes *when the
browser receives* the answer, not *how the answer is computed* — same pipeline,
same prompts, same model, same sampling. Cost: one extra small DB read every
6 s while recovering, and `query_log` rows now double as recovery records
(they were already written on every turn).

**bitsandbytes 4-bit vs the alternatives** — the honest accounting:

| vs | Quality | Speed |
|---|---|---|
| 4B bf16 (ideal) | NF4 4-bit loses a little fidelity in principle — but bf16 **does not fit** on this card, so it's not a real alternative | bnb kernels are less optimized than Marlin (vLLM warns about this); measured 60–66 tok/s is comfortably usable |
| 4B AWQ/GPTQ | those produce **NaN** here — bnb is the only 4-bit that computes correctly on this stack | AWQ-Marlin would be faster *if it worked*; it doesn't |
| 1B bf16 (yesterday's prod) | **Large quality gain**: grounding 0.86–1.50 with up to 15 clean citations, correct out-of-corpus refusals, no invented labels observed; runs the full v5.4.0 prompt (the 1B needed the stripped-down compact prompt) | Slower: ~6–11 s vs ~2 s per guidance turn |
| 8B AWQ (previous prod) | To be settled by a 30-question A/B (open) | Comparable single-query (~6 s both); 4B leaves ~2.5 GB more VRAM for KV → better concurrency headroom |

**Full v5.4.0 prompt vs compact**: no downside observed on 4B — the strongest
grounding score of the session (1.50) came under the full prompt. The compact
profile remains available (`NOVIQ_PROMPT_PROFILE=compact`) as a 1B fallback.

## 8. Follow-ups (priority order)

1. **Proper token clamp in `vllm.rs`** (removes the 400 class for good;
   headroom then becomes belt-and-suspenders).
2. **30-question A/B vs Qwen3-8B-AWQ** on the indexed corpus — the go/no-go
   for keeping 4B long-term.
3. Map-reduce latency trims (§6C) — cheapest first: lower map-pass
   `max_tokens`, scope ActionItems to named instruments.
4. WebSocket delivery for live tokens (replaces SSE-over-tunnel).
5. `query_log` retention policy now that it doubles as the recovery store.
