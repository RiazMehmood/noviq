#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# REFERENCE COPY of the VM's live /opt/noviq/gpu-reindex.sh — NOT executed from
# the repo. Like deploy.sh, this script is out of tree: the systemd path unit
# runs the copy on the VM. Editing this file changes nothing until it is copied
# across by hand. It lives here so the logic is version-controlled and reviewable.
#
# Snapshot updated 2026-08-04, after a run aborted at the canary having
# re-indexed exactly ONE document, on CPU:
#
#   0. ECHO_EMBED_GPU WAS ONLY EVER SET IN THE DROP-IN. The app force-loads
#      .env at startup, so a drop-in value is clobbered by any key .env also
#      defines — which is precisely why ORT_DYLIB_PATH is sed'd into .env two
#      lines below. `ECHO_EMBED_GPU=0` was added to .env some time after the
#      2026-07-29 run, and from that moment the drop-in's `=1` was dead: the
#      embedder never requested CUDA, ort never registered the CUDA EP, and the
#      canary waited out its full 300s for a line that could not appear.
#      Set it in .env alongside ORT_DYLIB_PATH; .env.bak restores both on exit.
#      LESSON: a drop-in Environment= is only load-bearing for keys absent
#      from .env. Adding a key to .env silently disables its drop-in override.
#
# Snapshot taken 2026-07-29, after fixing TWO defects that between them made a
# GPU re-index either abort immediately or stop after one document:
#
#   1. CANARY TIMING — it slept a fixed 14s then grepped the log for the CUDA
#      execution-provider line. The embedder loads ON FIRST USE and the canary
#      source (the largest document) parses for ~60s first, so the log was
#      still empty and a healthy run was cancelled with chat already down.
#      Now polls every 5s up to CANARY_MAX (default 300s).
#
#   2. COMPLETION SIGNAL — it watched the unembedded-chunk count, which is
#      BLIND while a document parses (no chunks exist yet). Between documents
#      it read 0, and three such polls ended the run 65s in, restoring CPU mode
#      and killing 13 of 17 in-flight re-indexes. Now every admin source must
#      be `indexed` with `indexed_at` (MILLISECONDS) newer than RUN_START_MS.
#      MAX_WAIT 1800 -> 5400 to cover parse + embed, not embed alone.
#
# Earlier snapshot (2026-07-26): repointed NV/ORT_GPU/CAPI at the rebuilt
# /opt/noviq/ort-gpu/ and added the preflight guard that aborts BEFORE stopping
# vLLM when the GPU runtime is missing or incomplete.
# ─────────────────────────────────────────────────────────────────────────────
# gpu-reindex.sh — one-click GPU re-embed of the admin corpus.
#
# Flow: banner up -> stop vLLM (free VRAM) -> flip noviq-server to GPU ONNX
# Runtime -> re-index all admin sources (embeddings run on the A40) -> restore
# CPU ONNX + restart vLLM -> banner down.
#
# SAFETY: a `trap ... EXIT` ALWAYS restores the CPU+vLLM known-good state, so any
# failure/timeout self-heals — chat is never left down or stuck in GPU mode.
#
# Run as root (systemd oneshot) or: sudo bash /opt/noviq/gpu-reindex.sh
set -uo pipefail

DATA=/opt/noviq/data
ENVF=/opt/noviq/echoAI/.env
ENV_BAK=/opt/noviq/echoAI/.env.bak.gpu-reindex
ORT_CPU=/opt/noviq/.ort-runtime/libonnxruntime.so.1.20.1
SENTINEL="$DATA/DEPLOYING"
TRIGGER="$DATA/GPU_REINDEX_REQUESTED"
DROPIN_DIR=/etc/systemd/system/noviq-server.service.d
DROPIN="$DROPIN_DIR/gpu-embed.conf"
API=http://127.0.0.1:8080
PSQL=/opt/noviq/data/pg/install/17.10.0/bin/psql
PGPASS=/opt/noviq/data/pg/.pgpass
LOG=/opt/noviq/gpu-reindex.log
SRC_PREFIX='/opt/noviq/data/uploads/admin/%'
MAX_WAIT=5400          # 90-min cap: full-corpus PARSE + embed, not embed alone
NV=/opt/noviq/ort-gpu/nvidia
ORT_GPU=/opt/noviq/ort-gpu/onnxruntime/lib/libonnxruntime.so.1.20.1
CAPI=/opt/noviq/ort-gpu/onnxruntime/lib
COMPAT=/opt/noviq/vllm/cuda-compat/usr/local/cuda-12.4/compat
LDP="$COMPAT:$NV/cublas/lib:$NV/cudnn/lib:$NV/cuda_runtime/lib:$NV/cufft/lib:$NV/curand/lib:$NV/cuda_nvrtc/lib:$NV/nvjitlink/lib:$CAPI"

exec >>"$LOG" 2>&1

# ── preflight: GPU runtime must be present and complete ─────────────────────
# The 2026-07-25 disk cleanup deleted the previous GPU runtime (ort-gpu-spike).
# The script kept running, the server fell back to the CPU execution provider
# (embedder.rs appends CPUExecutionProvider deliberately), and the whole
# "GPU re-embed" produced CPU embeddings with no error anywhere. Fail loudly
# and BEFORE touching vLLM or the env, so a missing runtime costs nothing.
preflight_fail() {
  echo "$(date -Is) PREFLIGHT FAILED: $*" >&2
  echo "$(date -Is) PREFLIGHT FAILED: $*"
  echo "$(date -Is) aborting WITHOUT stopping vLLM or modifying .env"
  exit 90
}
[ -f "$ORT_GPU" ] || preflight_fail "GPU onnxruntime missing: $ORT_GPU"
[ -f "$CAPI/libonnxruntime_providers_cuda.so" ] ||   preflight_fail "CUDA execution provider missing: $CAPI/libonnxruntime_providers_cuda.so"
for _c in cublas cudnn cuda_runtime cufft curand cuda_nvrtc nvjitlink; do
  [ -d "$NV/$_c/lib" ] || preflight_fail "CUDA component missing: $NV/$_c/lib"
done
if _missing=$(LD_LIBRARY_PATH="$LDP" ldd "$CAPI/libonnxruntime_providers_cuda.so" 2>/dev/null | grep "not found"); then
  preflight_fail "unresolved CUDA deps:
$_missing"
fi
echo "$(date -Is) preflight: GPU runtime OK ($ORT_GPU)"

echo "==================== $(date -Is) gpu-reindex START ===================="

log() { echo "$(date -Is) $*"; }
gpumem() { nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader 2>/dev/null; }

restore() {
  local rc=$?
  log "RESTORE (exit=$rc): reverting to CPU embed + vLLM"
  # revert ORT_DYLIB_PATH in .env back to the CPU lib (from backup if present)
  if [ -f "$ENV_BAK" ]; then cp -f "$ENV_BAK" "$ENVF"; else
    sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_CPU#" "$ENVF"; fi
  rm -f "$DROPIN"
  systemctl daemon-reload
  systemctl restart noviq-server
  systemctl start noviq-vllm
  rm -f "$SENTINEL" "$TRIGGER"
  log "RESTORE done. GPU now: $(gpumem)"
  log "==================== $(date -Is) gpu-reindex END (exit=$rc) ===================="
}
trap restore EXIT

pgport() {
  local pass p; pass="$(cut -d: -f5 "$PGPASS")"
  for p in $(ss -tlnp 2>/dev/null | grep postgres | grep -oE "127.0.0.1:[0-9]+" | grep -v ":8080" | cut -d: -f2); do
    PGPASSWORD="$pass" "$PSQL" -h 127.0.0.1 -p "$p" -U postgres -d echo -tAc "select 1 from sources limit 1" >/dev/null 2>&1 && { echo "$p"; return 0; }
  done
}
pq() {
  local port; port=$(pgport)
  PGPASSWORD="$(cut -d: -f5 "$PGPASS")" "$PSQL" -h 127.0.0.1 -p "$port" -U postgres -d echo -tAc "$1" 2>/dev/null
}

# ── 1) banner up ─────────────────────────────────────────────────────────────
touch "$SENTINEL"
log "banner up; GPU before: $(gpumem)"

# ── 2) free VRAM ─────────────────────────────────────────────────────────────
log "stopping vLLM..."
systemctl stop noviq-vllm
sleep 4
log "GPU after vLLM stop: $(gpumem)"

# ── 3) flip noviq-server to the GPU ONNX Runtime ─────────────────────────────
# ORT_DYLIB_PATH MUST be set in .env (the app force-loads .env at startup, so a
# systemd drop-in value is clobbered — proven runbook 2026-07-01). LD_LIBRARY_PATH
# must be in the drop-in (dynamic linker reads it at exec, before .env is loaded).
# ECHO_EMBED_GPU gets the SAME treatment as ORT_DYLIB_PATH: .env carries
# `ECHO_EMBED_GPU=0` for normal serving, and .env wins over the drop-in, so
# setting it only in the drop-in silently CPU-embeds (2026-08-04).
cp -f "$ENVF" "$ENV_BAK"
sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_GPU#" "$ENVF"
grep -q '^ORT_DYLIB_PATH=' "$ENVF" || echo "ORT_DYLIB_PATH=$ORT_GPU" >> "$ENVF"
sed -i "s#^ECHO_EMBED_GPU=.*#ECHO_EMBED_GPU=1#" "$ENVF"
grep -q '^ECHO_EMBED_GPU=' "$ENVF" || echo "ECHO_EMBED_GPU=1" >> "$ENVF"
mkdir -p "$DROPIN_DIR"
cat > "$DROPIN" <<CONF
[Service]
Environment=LD_LIBRARY_PATH=$LDP
Environment=ECHO_EMBED_GPU=1
CONF
systemctl daemon-reload
systemctl restart noviq-server
log "noviq-server restarted in GPU-embed mode; waiting for /api/health..."
for _ in $(seq 1 60); do curl -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 2; done

# ── 4) login ─────────────────────────────────────────────────────────────────
AU=$(grep -oP '^NOVIQ_ADMIN_USER=\K.*' "$ENVF")
AP=$(grep -oP '^NOVIQ_ADMIN_PASS=\K.*' "$ENVF")
TOKEN=$(curl -sf "$API/api/login" -H 'content-type: application/json' \
        -d "{\"username\":\"$AU\",\"password\":\"$AP\"}" | grep -oP '"token":"\K[^"]+')
[ -n "${TOKEN:-}" ] || { log "FATAL: login failed — aborting (trap restores)"; exit 1; }

# ── 5) enumerate admin sources + resolve a manageable folder id ──────────────
mapfile -t SIDS < <(pq "select id from sources where file_path like '$SRC_PREFIX'")
FOLDER=$(pq "select id from indexed_folders limit 1")
log "sources=${#SIDS[@]} folder=$FOLDER"
[ "${#SIDS[@]}" -ge 1 ] || { log "FATAL: no sources found — aborting"; exit 1; }

# ── 6) CANARY: reindex ONE source, confirm the CUDA EP registered ────────────
RUN_START_MS=$(( $(date +%s) * 1000 ))   # sources indexed before this are stale
log "canary reindex ${SIDS[0]}"
curl -sf "$API/api/retry_folder_source" -H "authorization: Bearer $TOKEN" \
     -H 'content-type: application/json' \
     -d "{\"sourceId\":\"${SIDS[0]}\",\"folderId\":\"$FOLDER\"}" >/dev/null
# 2026-07-29: a FIXED 14s sleep aborted a perfectly good run. The embedder
# loads ON FIRST USE, and the canary source (the largest document) parses for
# ~60s before the first embedding call, so the log was still empty and the run
# was cancelled with chat already down. Poll instead, and stop the moment
# either verdict appears. Override the ceiling with CANARY_MAX (seconds).
CANARY_MAX=${CANARY_MAX:-300}
CANARY_VERDICT=""
for _ in $(seq 1 $((CANARY_MAX / 5))); do
  sleep 5
  JL=$(journalctl -u noviq-server --since "-${CANARY_MAX}s" --no-pager)
  # TRUE success = ort's own "Successfully registered `CUDAExecutionProvider`".
  # The provider NAME also appears in the FAILURE line ("...not enabled in this
  # build") — so match the success/failure phrases, never the bare provider name.
  if grep -qi 'Successfully registered .CUDAExecutionProvider' <<<"$JL"; then
    CANARY_VERDICT=ok; break
  fi
  # `Embedding model ready (gpu=false)` (embedder.rs) is printed unconditionally
  # once the model loads and settles the question on its own: gpu=false means
  # ECHO_EMBED_GPU never reached the process, so the CUDA EP was never even
  # requested and no registration line will EVER appear. Without this the run
  # burns the full CANARY_MAX with chat down waiting on nothing (2026-08-04).
  if grep -qiE 'not enabled in this build|Successfully registered .CPUExecutionProvider|libcublasLt|cannot open shared object|Embedding model ready \(gpu=false\)' <<<"$JL"; then
    CANARY_VERDICT=fail; break
  fi
done
case "$CANARY_VERDICT" in
  ok)   log "CANARY OK: CUDAExecutionProvider actually registered — embeddings on GPU." ;;
  fail) log "CANARY FAIL: CPU fallback (CUDA EP not active) — aborting so we don't CPU-embed with chat down."; exit 1 ;;
  *)    log "CANARY: no decisive EP log line after ${CANARY_MAX}s; GPU=$(gpumem) — aborting to be safe." ; exit 1 ;;
esac

# ── 7) reindex the rest ──────────────────────────────────────────────────────
for sid in "${SIDS[@]:1}"; do
  curl -sf "$API/api/retry_folder_source" -H "authorization: Bearer $TOKEN" \
       -H 'content-type: application/json' \
       -d "{\"sourceId\":\"$sid\",\"folderId\":\"$FOLDER\"}" >/dev/null \
    && log "triggered $sid" || log "WARN trigger failed $sid"
done

# ── 8) wait for embeddings to catch up ───────────────────────────────────────
# NOTE: at trigger time every chunk is already embedded, so unembedded==0 is
# true immediately. We must first SEE re-index churn (unembedded>0 OR the total
# chunk count change vs baseline) before trusting a return-to-0 as "complete".
base_tot=$(pq "select count(*) from chunks")
log "baseline total chunks=$base_tot; waiting for every source to finish re-indexing"
# 2026-07-29: completion used to be inferred from the unembedded-chunk count.
# That signal is BLIND while a document is parsing (no chunks exist yet), so
# it read 0 between documents and declared victory 65s in, restoring CPU mode
# and killing 13 of 17 in-flight re-indexes. Ask the sources table instead:
# every admin source must be 'indexed' AND stamped after this run began.
# RUN_START_MS is set before the canary trigger. `indexed_at` is MILLISECONDS.
stale=0; stable=0; start=$(date +%s)
while :; do
  stale=$(pq "select count(*) from sources where file_path like '$SRC_PREFIX' and (indexing_status <> 'indexed' or coalesce(indexed_at,0) < $RUN_START_MS)")
  un=$(pq "select count(*) from chunks c left join chunk_embeddings e on e.chunk_id=c.id where e.chunk_id is null")
  tot=$(pq "select count(*) from chunks")
  log "not-yet-reindexed=${stale:-?} unembedded=${un:-?} total=${tot:-?} (base=$base_tot) gpu=$(gpumem)"
  if [ "${stale:-1}" = "0" ] && [ "${un:-1}" = "0" ] && [ "${tot:-0}" -gt 0 ]; then
    stable=$((stable+1))
  else
    stable=0
  fi
  [ "$stable" -ge 2 ] && { log "REINDEX COMPLETE (all sources re-indexed; embeddings caught up)"; break; }
  [ $(( $(date +%s) - start )) -gt "$MAX_WAIT" ] && { log "TIMEOUT after ${MAX_WAIT}s with ${stale:-?} source(s) unfinished — restoring anyway"; break; }
  sleep 15
done
# trap restore() runs on exit -> CPU ONNX + vLLM back, banner down.
