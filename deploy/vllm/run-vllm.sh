#!/usr/bin/env bash
# F016 (T011) — launch the vLLM OpenAI-compatible server (loopback only).
# Invoked by noviq-vllm.service. Reads tunables from the server's .env so the
# operator changes one file. See contracts/vllm-openai-api.md.
set -euo pipefail

VLLM_HOME="${VLLM_HOME:-/opt/noviq/vllm}"
VENV="$VLLM_HOME/venv310"
REPO="${REPO:-/opt/noviq/echoAI}"

# Load operator tunables (NOVIQ_VLLM_*). EnvironmentFile in the unit also injects
# these; sourcing here lets the script run standalone for debugging too.
[ -f "$REPO/.env" ] && set -a && . "$REPO/.env" && set +a || true

MODEL="${NOVIQ_VLLM_MODEL:-Qwen/Qwen3-8B-AWQ}"
PORT="${NOVIQ_VLLM_PORT:-8000}"
MAX_LEN="${NOVIQ_VLLM_MAX_MODEL_LEN:-8192}"     # FR-010 ceiling (>= ~6k floor); tune by load test
# vLLM serves a window WIDER than the server's prompt+gen budget (MAX_LEN). The
# Rust side budgets prompt+completion to MAX_LEN using ITS token estimate; vLLM
# enforces prompt+max_tokens <= served window using the EXACT Qwen tokenizer. The
# two counts differ by a few hundred tokens on dense legal text, so without slack
# a near-full prompt overflows the window and vLLM 400s ("maximum context length
# is N tokens"). The headroom absorbs that mismatch. (Proper long-term fix: clamp
# max_tokens against the real prompt count in vllm.rs — tracked as a follow-up.)
SERVE_LEN=$(( MAX_LEN + ${NOVIQ_VLLM_LEN_HEADROOM:-2048} ))
GPU_UTIL="${NOVIQ_VLLM_GPU_UTIL:-0.85}"          # fraction of TOTAL 12GB. NOTE (Spike-0): util
                                                 # counts other procs' VRAM too — keep noviq-server's
                                                 # GGUF unloaded when engine=vllm (else lower this).
MAX_SEQS="${NOVIQ_VLLM_MAX_NUM_SEQS:-16}"        # admits the 10-user target with burst margin
QUANT="${NOVIQ_VLLM_QUANT:-awq_marlin}"          # awq_marlin | awq | gptq_marlin | none (unquantized
                                                 # bf16, e.g. google/gemma-3-1b-it) — match NOVIQ_VLLM_MODEL
# ⚠️ NEVER set this back to plain `awq`. Passing `--quantization awq` EXPLICITLY
# disables vLLM's automatic upgrade to the Marlin INT4 kernel; it logs
#   "Detected that the model can run with awq_marlin, however you specified
#    quantization=awq explicitly, so forcing awq"
# and then decodes at ~15% of the card's achievable memory bandwidth. That line
# sat in every startup log from 2026-06-28 to 2026-08-12 and cost a 5.9x decode
# regression in production (13.8 -> 81.3 tok/s once fixed; a production-shaped
# 4.4k-prompt/479-token request went 36.2 s -> 7.7 s). Measured, see CHANGELOG
# 2026-08-12. Trade-off is real but tiny: Marlin's PREFILL is ~27% slower
# (4,804 -> 3,516 tok/s), and prefill is under 2% of request latency.
QUANT_ARGS=()
[ "$QUANT" != "none" ] && QUANT_ARGS=(--quantization "$QUANT")
[ "$QUANT" = "bitsandbytes" ] && QUANT_ARGS+=(--load-format bitsandbytes)

# ── Speculative decoding (OPT-IN, default OFF) ───────────────────────────────
# `ngram` is the draft-model-FREE variant: it guesses the next tokens by copying
# a matching run of text out of the prompt itself, then the real model verifies
# them in ONE forward pass. Wrong guesses are discarded, so **output is bit-identical
# to not using it** — only the latency changes. Costs no extra VRAM (no draft model),
# which matters because the 12 GB slice has ~2 GB free.
#
# Why it suits Noviq specifically: RAG answers quote retrieved passages verbatim,
# so the lookup hits often. Why it is OFF by default: the win is entirely workload-
# dependent (quotation flies, original prose gets nothing) and a miss costs a little
# wasted verification, so it must be MEASURED per deployment before being trusted.
# Flip it in .env, restart, compare `vllm:time_per_output_token_seconds` before/after.
#
#   NOVIQ_VLLM_SPEC=ngram     # enable   (empty/unset = OFF)
#   NOVIQ_VLLM_SPEC=          # disable  <- the reset switch
#
# vLLM 0.8.5 takes these as one JSON blob via --speculative-config, and its V1
# engine supports ngram and eagle ONLY (arg_utils.py:1423) — any other method
# silently falls back off V1.
SPEC="${NOVIQ_VLLM_SPEC:-}"                          # "" = off | ngram
SPEC_TOKENS="${NOVIQ_VLLM_SPEC_TOKENS:-5}"           # tokens guessed per step
SPEC_MIN="${NOVIQ_VLLM_SPEC_LOOKUP_MIN:-3}"          # shortest match to copy from
SPEC_MAX="${NOVIQ_VLLM_SPEC_LOOKUP_MAX:-8}"          # longest match to copy from
SPEC_ARGS=()
if [ "$SPEC" = "ngram" ]; then
  SPEC_ARGS=(--speculative-config "{\"method\":\"ngram\",\"num_speculative_tokens\":$SPEC_TOKENS,\"prompt_lookup_min\":$SPEC_MIN,\"prompt_lookup_max\":$SPEC_MAX}")
fi

# CUDA-12 forward-compat: prepend the extracted libcuda.so.550 so the CUDA-12 wheel
# runs on the host R470 kernel driver (Spike-0). No host driver change.
COMPAT_LIB="$(dirname "$(find "$VLLM_HOME/cuda-compat" -name 'libcuda.so*' 2>/dev/null | head -1)")"
export LD_LIBRARY_PATH="${COMPAT_LIB:+$COMPAT_LIB:}${LD_LIBRARY_PATH:-}"

# Fully offline (FR-006): weights resolve from the local HF cache, no network fetch.
export HF_HUB_OFFLINE=1
export HF_HOME="${HF_HOME:-/opt/noviq/model-library/hf-cache}"

echo "vllm serve $MODEL :$PORT serve_len=$SERVE_LEN (budget=$MAX_LEN) util=$GPU_UTIL seqs=$MAX_SEQS quant=$QUANT spec=${SPEC:-off}"
# kv-cache auto (fp16) + chunked prefill: fp8 KV crashes Triton fp8e4nv on
# sm_86 under the V1 chunked-prefill engine (F016 cutover finding) — this
# mirrors the live /opt/noviq/vllm-scripts copy, reconciled 2026-08-12.
exec "$VENV/bin/vllm" serve "$MODEL" \
  --host 127.0.0.1 --port "$PORT" \
  --served-model-name "$MODEL" \
  "${QUANT_ARGS[@]}" \
  "${SPEC_ARGS[@]}" \
  --kv-cache-dtype auto \
  --max-model-len "$SERVE_LEN" \
  --gpu-memory-utilization "$GPU_UTIL" \
  --max-num-seqs "$MAX_SEQS" \
  --enable-chunked-prefill --disable-log-requests
