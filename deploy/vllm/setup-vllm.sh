#!/usr/bin/env bash
# F016 (T010) — one-time, OUT-OF-TREE provisioning of the vLLM sidecar.
#
# Lives under /opt/noviq/vllm so it SURVIVES `git reset --hard` in the poll-deploy
# (mirrors the F006 ingestion/OCR sidecars + the F011 bundled-PG). The Rust/FE
# poll-deploy never rebuilds this — run it by hand once (and again only to bump
# vLLM / re-download a model).
#
# Encodes the Spike-0 findings (2026-06-28, see specs/016-vllm-serving/research.md
# + VLLM_SPIKE_RESUME.md): R470 vGPU needs CUDA-12 *forward-compat*; system python
# 3.8 is too old (use uv's 3.10); transformers MUST be pinned to 4.51.3 or vLLM
# 0.8.5's tokenizer cache breaks; triton needs setuptools at runtime.
#
# Idempotent: re-running upgrades pins and re-checks the model cache without harm.
set -euo pipefail

VLLM_HOME="${VLLM_HOME:-/opt/noviq/vllm}"
VENV="$VLLM_HOME/venv310"
COMPAT_DIR="$VLLM_HOME/cuda-compat"           # extracted libcuda.so.550 (forward-compat)
MODEL_LIB="${MODEL_LIB:-/opt/noviq/model-library}"
VLLM_VERSION="${VLLM_VERSION:-0.8.5}"
CUDA_COMPAT_PKG="${CUDA_COMPAT_PKG:-cuda-compat-12-4}"  # match the wheel's CUDA (12.4)
# Both 4-bit builds are fetched so the A/B (T029/T030) can pick the launch winner.
MODELS=("Qwen/Qwen3-8B-AWQ" "RedHatAI/Qwen3-8B-quantized.w4a16")

mkdir -p "$VLLM_HOME" "$MODEL_LIB"
cd "$VLLM_HOME"

echo "=== [1/5] uv (standalone, no sudo) ==="
if ! command -v uv >/dev/null 2>&1; then
  curl -LsSf https://astral.sh/uv/install.sh | sh
fi
UV="$(command -v uv || echo "$HOME/.local/bin/uv")"

echo "=== [2/5] python 3.10 venv (system 3.8 is too old for vLLM>=0.8.5) ==="
[ -d "$VENV" ] || "$UV" venv --python 3.10 "$VENV"

echo "=== [3/5] vLLM $VLLM_VERSION + PINNED deps into the venv ==="
# Pin transformers/tokenizers (Spike-0: uv otherwise pulls transformers 5.x which
# breaks vLLM 0.8.5's get_cached_tokenizer). setuptools+wheel are needed by triton
# at runtime (its absence is the "Engine core init failed" trap).
VIRTUAL_ENV="$VENV" "$UV" pip install \
  "vllm==$VLLM_VERSION" \
  "transformers==4.51.3" \
  "tokenizers==0.21.4" \
  setuptools wheel

echo "=== [4/5] CUDA-12 forward-compat userspace driver (extracted locally, no sudo) ==="
# Extract the compat .deb into VLLM_HOME so libcuda.so.550 rides in LD_LIBRARY_PATH
# at serve time WITHOUT touching the host R470 kernel driver.
if [ ! -d "$COMPAT_DIR" ]; then
  TMP="$(mktemp -d)"
  ( cd "$TMP"
    if command -v apt-get >/dev/null 2>&1; then
      apt-get download "$CUDA_COMPAT_PKG" 2>/dev/null \
        || { echo "apt-get download failed — fetch ${CUDA_COMPAT_PKG}_*.deb manually into $TMP"; exit 1; }
    fi
    DEB="$(ls "${CUDA_COMPAT_PKG}"*.deb 2>/dev/null | head -1)"
    [ -n "$DEB" ] || { echo "no ${CUDA_COMPAT_PKG} .deb found in $TMP"; exit 1; }
    dpkg-deb -x "$DEB" "$COMPAT_DIR" )
  rm -rf "$TMP"
fi
# Resolve the compat lib dir (…/usr/local/cuda-12.4/compat) for run-vllm.sh / the unit.
COMPAT_LIB="$(dirname "$(find "$COMPAT_DIR" -name 'libcuda.so*' 2>/dev/null | head -1)")"
echo "compat lib dir: ${COMPAT_LIB:-NOT FOUND}"

echo "=== [5/5] download model weights (one-time; offline thereafter) ==="
export HF_HOME="${HF_HOME:-$MODEL_LIB/hf-cache}"
export HF_HUB_OFFLINE=0
for m in "${MODELS[@]}"; do
  echo "  - $m"
  "$VENV/bin/python" - "$m" <<'PY'
import sys
from huggingface_hub import snapshot_download
snapshot_download(sys.argv[1])
print("  cached:", sys.argv[1])
PY
done

cat <<EOF

=== setup-vllm.sh DONE ===
venv:        $VENV
compat lib:  ${COMPAT_LIB:-<unresolved>}
HF cache:    $HF_HOME
models:      ${MODELS[*]}

Next:
  1) Put these in /opt/noviq/echoAI/.env (tune per load test, T015/T017):
       NOVIQ_VLLM_URL=http://127.0.0.1:8000
       NOVIQ_VLLM_MODEL=Qwen/Qwen3-8B-AWQ
       NOVIQ_VLLM_MAX_MODEL_LEN=8192
       NOVIQ_VLLM_GPU_UTIL=0.85
       NOVIQ_VLLM_MAX_NUM_SEQS=16
       NOVIQ_VLLM_QUANT=awq
       HF_HOME=$HF_HOME
  2) sudo cp deploy/vllm/noviq-vllm.service /etc/systemd/system/
     sudo systemctl daemon-reload && sudo systemctl enable --now noviq-vllm
  3) curl -s 127.0.0.1:8000/health   # 200 once the model is resident (cold start 10–30s)
EOF
