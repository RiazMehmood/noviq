#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# gpu-hold.sh — enter GPU-embedding mode ONCE and HOLD it for a bulk corpus run.
#
# REFERENCE COPY — runs from /opt/noviq on the VM. Editing it here changes
# nothing until it is copied across. Run as root:  sudo bash /opt/noviq/gpu-hold.sh
# Undo with:                                       sudo bash /opt/noviq/gpu-release.sh
#
# WHY THIS EXISTS ALONGSIDE THE POLLER (gpu-index-auto.sh):
# The poller decides afresh every minute, so during a long run it can toggle —
# and entering GPU mode costs a server restart. `add_indexed_folder` walks its
# directory LAZILY, so that restart kills the walk, and files not yet reached
# have NO source rows for `resume_interrupted_indexing` to re-queue. HEALTH
# stopped at 46/132 exactly this way (2026-08-15). One switch up front, held for
# the whole run, removes the toggle entirely.
#
# The poller's timer is STOPPED here so the two cannot fight. The pause sentinel
# (GPU_MODE_DISABLED) is deliberately LEFT IN PLACE: if the timer ever comes back
# by itself (reboot), the poller sees pause + our GPU_MODE_ACTIVE marker and
# RELEASES — chat returns and indexing merely falls back to CPU. The failure
# direction is the safe one.
#
# The env/drop-in split below is copied deliberately from gpu-index-auto.sh and
# must stay identical: ORT_DYLIB_PATH / ECHO_EMBED_GPU go in .env (the app
# force-loads it at startup and it CLOBBERS drop-in values), LD_LIBRARY_PATH must
# be the systemd drop-in (the linker reads it at exec, before .env). Getting this
# backwards ran an entire "GPU" re-embed on CPU (2026-08-04).
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail

DATA=/opt/noviq/data
ENVF=/opt/noviq/echoAI/.env
ENV_BAK=/opt/noviq/echoAI/.env.bak.gpu-hold
ORT_GPU=/opt/noviq/ort-gpu/onnxruntime/lib/libonnxruntime.so.1.20.1
CAPI=/opt/noviq/ort-gpu/onnxruntime/lib
NV=/opt/noviq/ort-gpu/nvidia
COMPAT=/opt/noviq/vllm/cuda-compat/usr/local/cuda-12.4/compat
LDP="$COMPAT:$NV/cublas/lib:$NV/cudnn/lib:$NV/cuda_runtime/lib:$NV/cufft/lib:$NV/curand/lib:$NV/cuda_nvrtc/lib:$NV/nvjitlink/lib:$CAPI"
DROPIN_DIR=/etc/systemd/system/noviq-server.service.d
DROPIN="$DROPIN_DIR/gpu-embed.conf"
BANNER="$DATA/DEPLOYING"        # /api/health reports "deploying"; the browser explains itself
STATE="$DATA/GPU_MODE_ACTIVE"   # same marker the poller uses, on purpose (see above)
HOLD="$DATA/GPU_HOLD"           # this hold, specifically — so release knows who to undo
LOG=/opt/noviq/gpu-hold.log
API=http://127.0.0.1:8080

[ "$(id -u)" = 0 ] || { echo "gpu-hold.sh must run as root (sudo bash $0)"; exit 1; }

exec >>"$LOG" 2>&1
log() { echo "$(date -Is) $*" ; }
gpumem() { nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader 2>/dev/null; }

log "================ gpu-hold START ================"

# Preflight FIRST: a missing CUDA runtime must cost nothing. The 2026-07-25
# cleanup deleted it once, the run continued, and the CPU provider silently
# served the whole "GPU" re-embed.
[ -f "$ORT_GPU" ] || { log "preflight: GPU onnxruntime missing — ABORT, staying on CPU"; exit 1; }
[ -f "$CAPI/libonnxruntime_providers_cuda.so" ] || { log "preflight: CUDA EP missing — ABORT"; exit 1; }
if LD_LIBRARY_PATH="$LDP" ldd "$CAPI/libonnxruntime_providers_cuda.so" 2>/dev/null | grep -q "not found"; then
  log "preflight: unresolved CUDA deps — ABORT"; exit 1
fi

if [ -f "$STATE" ]; then
  log "already in GPU mode (since $(cat "$STATE" 2>/dev/null)) — nothing to do"
  touch "$HOLD"; exit 0
fi

# Stop the poller so it cannot toggle underneath the run.
systemctl stop noviq-gpu-index.timer
log "poller timer stopped"

log "ENTER GPU mode; GPU before: $(gpumem)"
touch "$BANNER"
systemctl stop noviq-vllm
sleep 3
cp -f "$ENVF" "$ENV_BAK"
sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_GPU#" "$ENVF"
grep -q '^ORT_DYLIB_PATH=' "$ENVF" || echo "ORT_DYLIB_PATH=$ORT_GPU" >> "$ENVF"
sed -i "s#^ECHO_EMBED_GPU=.*#ECHO_EMBED_GPU=1#" "$ENVF"
grep -q '^ECHO_EMBED_GPU=' "$ENVF" || echo "ECHO_EMBED_GPU=1" >> "$ENVF"
mkdir -p "$DROPIN_DIR"
printf '[Service]\nEnvironment=LD_LIBRARY_PATH=%s\n' "$LDP" > "$DROPIN"
systemctl daemon-reload
systemctl restart noviq-server
for _ in $(seq 1 60); do curl -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 2; done

date '+%Y-%m-%d %H:%M:%S' > "$STATE"
touch "$HOLD"
# The banner comes off: the server is up and answering, it just has no chat model.
rm -f "$BANNER"
log "GPU mode HELD; GPU now: $(gpumem)"
log "start the corpus run now:  tmux new -s corpus -d /opt/noviq/run-corpus.sh"
log "release when done:         sudo bash /opt/noviq/gpu-release.sh"
