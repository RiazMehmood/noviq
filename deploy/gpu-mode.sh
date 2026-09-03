#!/usr/bin/env bash
# gpu-mode.sh — put noviq-server into GPU-embedding mode and LEAVE it there,
# so ordinary indexing from the admin console runs on the A40.
#
# WHY THIS EXISTS
#   gpu-reindex.sh is a one-shot: it stops vLLM, flips to the GPU ONNX Runtime,
#   RE-embeds sources that are already indexed, then restores everything. That
#   does not help when you are indexing *newly uploaded* documents — those go
#   through the normal upload → index path in the admin UI, which runs in the
#   already-running server, which loaded the CPU runtime at startup.
#
#   This script separates the two halves. `on` performs the same environment
#   flip and leaves the server in GPU mode; you then upload and index through
#   the UI as normal and the embeddings run on the GPU. `off` puts everything
#   back. Chat is DOWN in between — that is the deliberate trade: the 12 GB
#   A40 slice cannot hold vLLM's KV cache and the ONNX embedder at once
#   (~1.4 GB free with vLLM up, which is not enough).
#
# THE TWO PLACEMENT RULES (proven runbook 2026-07-01, do not "simplify")
#   ORT_DYLIB_PATH  -> .env         the app force-loads .env at startup, so a
#                                   systemd drop-in value gets clobbered.
#   LD_LIBRARY_PATH -> drop-in      the dynamic linker reads it at exec, long
#                                   before .env is parsed.
#
# SAFETY
#   * preflight: refuses to touch anything if the GPU runtime is missing or
#     incomplete (this is what silently CPU-embedded for two weeks in July).
#   * canary: after the restart, confirms ort actually logged
#     "Successfully registered CUDAExecutionProvider". If it fell back to CPU,
#     `on` restores immediately rather than leaving chat down for nothing.
#   * watchdog: `on` schedules an automatic `off` (default 120 min) so a
#     forgotten session cannot leave chat down indefinitely. `off` cancels it.
#
# USAGE   sudo bash /opt/noviq/gpu-mode.sh {on [minutes] | off | status}

set -uo pipefail

DATA=/opt/noviq/data
ENVF=/opt/noviq/echoAI/.env
ENV_BAK=/opt/noviq/echoAI/.env.bak.gpu-mode
ORT_CPU=/opt/noviq/.ort-runtime/libonnxruntime.so.1.20.1
ORT_DIR=/opt/noviq/ort-gpu
ORT_GPU=$ORT_DIR/onnxruntime/lib/libonnxruntime.so.1.20.1
CAPI=$ORT_DIR/onnxruntime/lib
NV=$ORT_DIR/nvidia
COMPAT=/opt/noviq/vllm/cuda-compat/usr/local/cuda-12.4/compat
LDP="$COMPAT:$NV/cublas/lib:$NV/cudnn/lib:$NV/cuda_runtime/lib:$NV/cufft/lib:$NV/curand/lib:$NV/cuda_nvrtc/lib:$NV/nvjitlink/lib:$CAPI"
DROPIN_DIR=/etc/systemd/system/noviq-server.service.d
DROPIN="$DROPIN_DIR/gpu-embed.conf"
# Planned-maintenance sentinel — NOT the deploy sentinel.
# Writing $DATA/DEPLOYING here (as gpu-reindex.sh does) makes the browser announce
# "Noviq is updating to a new version", which is false: nothing is being rebuilt,
# vLLM is simply stopped so the GPU can embed. The first line of this file is shown
# to users verbatim by /api/health -> ServerStatusBanner.
SENTINEL="$DATA/MAINTENANCE"
SENTINEL_MSG="Noviq is indexing documents — chat is paused and will return shortly."
STATEF="$DATA/GPU_MODE_SINCE"
WATCHDOG="$DATA/gpu-mode-watchdog.pid"
API=http://127.0.0.1:8080
LOG=/opt/noviq/gpu-mode.log

log() { echo "$(date -Is) $*" | tee -a "$LOG"; }

preflight() {
  [ -f "$ORT_GPU" ] || { log "PREFLIGHT FAIL: missing $ORT_GPU"; return 1; }
  [ -f "$CAPI/libonnxruntime_providers_cuda.so" ] || {
    log "PREFLIGHT FAIL: missing CUDA execution provider in $CAPI"; return 1; }
  local c
  for c in cublas cudnn cuda_runtime cufft curand cuda_nvrtc nvjitlink; do
    [ -d "$NV/$c/lib" ] || { log "PREFLIGHT FAIL: missing CUDA component $NV/$c/lib"; return 1; }
  done
  local missing
  missing=$(LD_LIBRARY_PATH="$LDP" ldd "$CAPI/libonnxruntime_providers_cuda.so" 2>/dev/null | grep "not found")
  [ -z "$missing" ] || { log "PREFLIGHT FAIL: unresolved deps:"$'\n'"$missing"; return 1; }
  log "preflight OK"
}

gpumem() { nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader 2>/dev/null; }

current_mode() {
  if [ -f "$DROPIN" ] && grep -q "^ORT_DYLIB_PATH=$ORT_GPU" "$ENVF" 2>/dev/null; then
    echo gpu
  else
    echo cpu
  fi
}

cancel_watchdog() {
  if [ -f "$WATCHDOG" ]; then
    kill "$(cat "$WATCHDOG")" 2>/dev/null
    rm -f "$WATCHDOG"
  fi
}

mode_off() {
  log "=== gpu-mode OFF ==="
  cancel_watchdog
  if [ -f "$ENV_BAK" ]; then
    cp -f "$ENV_BAK" "$ENVF"
  else
    sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_CPU#" "$ENVF"
  fi
  rm -f "$DROPIN" "$STATEF"
  systemctl daemon-reload
  systemctl restart noviq-server
  systemctl start noviq-vllm
  rm -f "$SENTINEL"
  log "restored: CPU embeddings + vLLM. GPU now: $(gpumem)"
}

mode_on() {
  local minutes="${1:-120}"
  if [ "$(current_mode)" = gpu ]; then
    log "already in GPU mode (since $(cat "$STATEF" 2>/dev/null)); nothing to do"
    return 0
  fi
  preflight || { log "aborting WITHOUT stopping vLLM or touching .env"; return 90; }

  log "=== gpu-mode ON (auto-off in ${minutes}m) ==="
  printf '%s\n' "$SENTINEL_MSG" > "$SENTINEL"   # browser shows this text verbatim
  log "stopping vLLM (chat goes down) ..."
  systemctl stop noviq-vllm
  sleep 4
  log "GPU after vLLM stop: $(gpumem)"

  cp -f "$ENVF" "$ENV_BAK"
  sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_GPU#" "$ENVF"
  grep -q '^ORT_DYLIB_PATH=' "$ENVF" || echo "ORT_DYLIB_PATH=$ORT_GPU" >> "$ENVF"
  mkdir -p "$DROPIN_DIR"
  cat > "$DROPIN" <<CONF
[Service]
Environment=LD_LIBRARY_PATH=$LDP
Environment=ECHO_EMBED_GPU=1
CONF
  systemctl daemon-reload
  systemctl restart noviq-server
  log "server restarting in GPU-embed mode; waiting for health ..."
  local i
  for i in $(seq 1 60); do curl -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 2; done

  # ── canary ────────────────────────────────────────────────────────────────
  # The embedder is lazy: it loads on first use, so nothing is logged until we
  # force an embedding. Hit a cheap authenticated-free path that embeds — if
  # none is available, fall back to waiting for the operator's first index.
  # Either way we only trust ort's own success line. The provider NAME also
  # appears in the FAILURE message ("not enabled in this build"), so match the
  # success/failure PHRASES, never the bare name.
  log "canary: waiting for the execution-provider decision ..."
  local jl decided=0
  for i in $(seq 1 20); do
    sleep 3
    jl=$(journalctl -u noviq-server --since '-90s' --no-pager 2>/dev/null)
    if grep -qi 'Successfully registered .CUDAExecutionProvider' <<<"$jl"; then
      log "CANARY OK: CUDAExecutionProvider registered — indexing will embed on GPU."
      decided=1; break
    elif grep -qiE 'not enabled in this build|Successfully registered .CPUExecutionProvider|libcublasLt|cannot open shared object' <<<"$jl"; then
      log "CANARY FAIL: CPU fallback detected — restoring rather than leaving chat down."
      mode_off
      return 91
    fi
  done
  if [ "$decided" -eq 0 ]; then
    log "CANARY: no EP line yet (embedder is lazy — it decides on first embed)."
    log "        Start your index now, then run: bash $0 status"
    log "        If it reports CPU fallback, run: bash $0 off"
  fi

  date -Is > "$STATEF"
  # ── watchdog: auto-restore so a forgotten session cannot strand chat ───────
  ( sleep $(( minutes * 60 ))
    if [ -f "$STATEF" ]; then
      log "WATCHDOG: ${minutes}m elapsed — restoring automatically"
      mode_off
    fi
  ) >/dev/null 2>&1 &
  echo $! > "$WATCHDOG"

  log "GPU mode ACTIVE. Chat is DOWN. GPU: $(gpumem)"
  log "Index your documents now, then run:  sudo bash $0 off"
}

mode_status() {
  local m; m=$(current_mode)
  echo "mode:        $m"
  echo "since:       $(cat "$STATEF" 2>/dev/null || echo n/a)"
  echo "vllm:        $(systemctl is-active noviq-vllm 2>/dev/null)"
  echo "server:      $(systemctl is-active noviq-server 2>/dev/null)"
  echo "ORT in .env: $(grep -oP '^ORT_DYLIB_PATH=\K.*' "$ENVF" 2>/dev/null)"
  echo "gpu:         $(gpumem)"
  if [ "$m" = gpu ]; then
    local jl; jl=$(journalctl -u noviq-server --since '-30m' --no-pager 2>/dev/null)
    if grep -qi 'Successfully registered .CUDAExecutionProvider' <<<"$jl"; then
      echo "provider:    CUDA (confirmed in log)"
    elif grep -qiE 'not enabled in this build|Successfully registered .CPUExecutionProvider' <<<"$jl"; then
      echo "provider:    CPU FALLBACK — run '$0 off', GPU mode is not working"
    else
      echo "provider:    undecided (embedder is lazy; index something first)"
    fi
  fi
}

case "${1:-status}" in
  on)     mode_on "${2:-120}" ;;
  off)    mode_off ;;
  status) mode_status ;;
  *)      echo "usage: $0 {on [minutes] | off | status}" >&2; exit 2 ;;
esac
