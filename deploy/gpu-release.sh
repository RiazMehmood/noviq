#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# gpu-release.sh — undo gpu-hold.sh: CPU embeddings, chat back, poller re-armed.
#
# REFERENCE COPY — runs from /opt/noviq on the VM.  sudo bash /opt/noviq/gpu-release.sh
#
# Safe to run at any moment, including mid-run: it only restores the runtime the
# server starts with. Indexing in flight is killed by the restart, but every file
# already has a source row, so `resume_interrupted_indexing` re-queues it — the
# lazy-walk hazard belongs to the folder SCAN, not to the files already
# registered. Whatever is still being scanned is picked up by run-corpus.sh on
# its next pass, which compares on-disk against indexed counts.
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail

DATA=/opt/noviq/data
ENVF=/opt/noviq/echoAI/.env
ENV_BAK=/opt/noviq/echoAI/.env.bak.gpu-hold
ORT_CPU=/opt/noviq/.ort-runtime/libonnxruntime.so.1.20.1
DROPIN=/etc/systemd/system/noviq-server.service.d/gpu-embed.conf
BANNER="$DATA/DEPLOYING"
STATE="$DATA/GPU_MODE_ACTIVE"
IDLE="$DATA/GPU_MODE_IDLE"
HOLD="$DATA/GPU_HOLD"
LOG=/opt/noviq/gpu-hold.log

[ "$(id -u)" = 0 ] || { echo "gpu-release.sh must run as root (sudo bash $0)"; exit 1; }

exec >>"$LOG" 2>&1
log() { echo "$(date -Is) $*" ; }
gpumem() { nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader 2>/dev/null; }

log "LEAVE GPU mode — restoring CPU embeddings and chat"
# Restore from the backup taken at hold time; the sed fallback covers a missing
# backup so this can never leave the box pointing at the GPU runtime.
if [ -f "$ENV_BAK" ]; then
  cp -f "$ENV_BAK" "$ENVF"
else
  sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_CPU#" "$ENVF"
  sed -i "s#^ECHO_EMBED_GPU=.*#ECHO_EMBED_GPU=0#" "$ENVF"
fi
rm -f "$DROPIN"
systemctl daemon-reload
systemctl restart noviq-server
systemctl start noviq-vllm
rm -f "$STATE" "$IDLE" "$BANNER" "$HOLD"
systemctl start noviq-gpu-index.timer
log "chat restored, poller timer re-armed; GPU now: $(gpumem)"
