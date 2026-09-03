#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# gpu-index-auto.sh — AUTOMATIC GPU indexing. Run every minute by
# noviq-gpu-index.timer. No operator action: a folder upload takes chat down and
# hands the GPU to the embedder; when indexing finishes, chat comes back.
#
# REFERENCE COPY — runs from /opt/noviq on the VM. Editing it here changes
# nothing until it is copied across.
#
# WHY A POLLER, not the long-running script this replaces: that one held the
# window open inside a `while` loop, and its canary wait ignored the release
# sentinel — so "end early" did nothing for up to fifteen minutes and chat
# stayed down (hit for real, 2026-08-14). Each run here is a short, stateless
# decision, so there is no loop to be stuck in and nothing to get wedged.
#
# WHY the two modes cannot coexist: the vGPU is an A40-12Q with 12,107 MiB, vLLM
# holds ~10,173 and leaves 878 free, against the ~1.5 GB the ONNX embedder needs
# for its CUDA context and weights. One or the other.
#
# WHY a restart is safe: switching runtimes is an ORT_DYLIB_PATH /
# LD_LIBRARY_PATH change, both read at exec, so it costs a server restart —
# which kills in-flight indexing. `resume_interrupted_indexing` re-queues those
# files at startup, and because the progress counter is only bumped AFTER a file
# finishes, a file killed mid-index is counted exactly once.
#
# TRIGGER: `indexed_folders.indexing_total > 0` (migration 026) — a batch is
# running. That fires only AFTER the upload has written every file to disk, so
# the restart can never interrupt a transfer.
# ─────────────────────────────────────────────────────────────────────────────
set -uo pipefail

DATA=/opt/noviq/data
ENVF=/opt/noviq/echoAI/.env
ENV_BAK=/opt/noviq/echoAI/.env.bak.gpu-index
ORT_CPU=/opt/noviq/.ort-runtime/libonnxruntime.so.1.20.1
ORT_GPU=/opt/noviq/ort-gpu/onnxruntime/lib/libonnxruntime.so.1.20.1
CAPI=/opt/noviq/ort-gpu/onnxruntime/lib
NV=/opt/noviq/ort-gpu/nvidia
COMPAT=/opt/noviq/vllm/cuda-compat/usr/local/cuda-12.4/compat
LDP="$COMPAT:$NV/cublas/lib:$NV/cudnn/lib:$NV/cuda_runtime/lib:$NV/cufft/lib:$NV/curand/lib:$NV/cuda_nvrtc/lib:$NV/nvjitlink/lib:$CAPI"
DROPIN_DIR=/etc/systemd/system/noviq-server.service.d
DROPIN="$DROPIN_DIR/gpu-embed.conf"
BANNER="$DATA/DEPLOYING"          # /api/health reports "deploying"; the browser explains itself
STATE="$DATA/GPU_MODE_ACTIVE"     # our own marker: are we currently in GPU mode
IDLE="$DATA/GPU_MODE_IDLE"        # consecutive idle polls
PAUSE="$DATA/GPU_MODE_DISABLED"   # touch to opt out entirely (no sudo needed)
PSQL=/opt/noviq/data/pg/install/17.10.0/bin/psql
PGPASS=/opt/noviq/data/pg/.pgpass
LOG=/opt/noviq/gpu-index-auto.log
API=http://127.0.0.1:8080
IDLE_POLLS=${IDLE_POLLS:-3}       # ~3 min quiet before handing the GPU back
# Batches smaller than this stay on CPU, with chat up. Entering and leaving GPU
# mode costs two server restarts and a couple of minutes of downtime, so for a
# handful of files the switch is a net loss — a 7-file upload took chat down for
# nothing (2026-08-14).
# Architect's call 2026-08-14: only skip the switch for FEWER THAN 5 files, i.e.
# anything from 5 upwards takes the GPU. (I had proposed 20 on cost grounds; the
# decision is to favour GPU use.)
MIN_BATCH=${MIN_BATCH:-5}

exec >>"$LOG" 2>&1
log() { echo "$(date -Is) $*"; }
gpumem() { nvidia-smi --query-gpu=memory.used,memory.free --format=csv,noheader 2>/dev/null; }

# ⚠️ The pause flag must RELEASE, not just stop deciding. It used to `exit 0`
# right here — so touching it while GPU mode was active froze the box with chat
# down and no way back short of an operator. It is the emergency handle; it has
# to be safe to pull at the worst moment. The release itself is below, once the
# helpers are defined.
PAUSED=0
[ -f "$PAUSE" ] && PAUSED=1

# Postgres port drifts on every server restart, so it is discovered, never
# assumed. PGCONNECT_TIMEOUT stops the probe hanging on the non-PG listeners.
pq() {
  local pass port out
  pass="$(cut -d: -f5 "$PGPASS" 2>/dev/null)" || return 1
  for port in $(ss -tln 2>/dev/null | grep -oE '127\.0\.0\.1:[0-9]+' | cut -d: -f2 | sort -un); do
    case "$port" in 631|8000|8080|8888) continue;; esac
    out=$(PGCONNECT_TIMEOUT=2 PGPASSWORD="$pass" "$PSQL" -w -h 127.0.0.1 -p "$port" \
          -U postgres -d echo -tAc "$1" 2>/dev/null) && { echo "$out"; return 0; }
  done
  return 1
}

batches=$(pq "select count(*) from indexed_folders where indexing_total > 0") || { log "no PG; skipping"; exit 0; }
files=$(pq   "select count(*) from sources where indexing_status in ('pending','processing')")
# The SIZE of the batch, not what is left of it. MIN_BATCH used to be compared
# against pending files, so a small-but-quick batch had already drained below the
# threshold by the first poll and never got the GPU — a 7-file folder logged
# "batch of 1 file(s) is below MIN_BATCH". Whether the switch is worth making is
# a question about the batch, not about the instant we happened to look.
batchsize=$(pq "select coalesce(sum(indexing_total),0) from indexed_folders where indexing_total > 0")
batches=${batches:-0}; files=${files:-0}; batchsize=${batchsize:-0}

enter_gpu_mode() {
  # Preflight FIRST: a missing CUDA runtime must cost nothing. The 2026-07-25
  # cleanup deleted it once, the run continued, and the CPU provider silently
  # served the whole "GPU" re-embed.
  [ -f "$ORT_GPU" ] || { log "preflight: GPU onnxruntime missing — staying on CPU"; return 1; }
  [ -f "$CAPI/libonnxruntime_providers_cuda.so" ] || { log "preflight: CUDA EP missing — staying on CPU"; return 1; }
  if LD_LIBRARY_PATH="$LDP" ldd "$CAPI/libonnxruntime_providers_cuda.so" 2>/dev/null | grep -q "not found"; then
    log "preflight: unresolved CUDA deps — staying on CPU"; return 1
  fi

  log "ENTER GPU mode (batches=$batches files=$files); GPU before: $(gpumem)"
  touch "$BANNER"
  systemctl stop noviq-vllm
  sleep 3
  cp -f "$ENVF" "$ENV_BAK"
  # ⚠️ Both keys go in .env: the app force-loads it at startup and it CLOBBERS
  # any drop-in value. LD_LIBRARY_PATH is the opposite — the linker reads it at
  # exec, before .env — so that one MUST be the drop-in. Getting this backwards
  # is what made an entire GPU re-embed run on CPU (2026-08-04).
  sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_GPU#" "$ENVF"
  grep -q '^ORT_DYLIB_PATH=' "$ENVF" || echo "ORT_DYLIB_PATH=$ORT_GPU" >> "$ENVF"
  sed -i "s#^ECHO_EMBED_GPU=.*#ECHO_EMBED_GPU=1#" "$ENVF"
  grep -q '^ECHO_EMBED_GPU=' "$ENVF" || echo "ECHO_EMBED_GPU=1" >> "$ENVF"
  mkdir -p "$DROPIN_DIR"
  printf '[Service]\nEnvironment=LD_LIBRARY_PATH=%s\n' "$LDP" > "$DROPIN"
  systemctl daemon-reload
  systemctl restart noviq-server
  # The restart drops in-flight indexing; the server re-queues it on boot.
  for _ in $(seq 1 60); do curl -sf "$API/api/health" >/dev/null 2>&1 && break; sleep 2; done
  echo 0 > "$IDLE"
  # Stamp WHEN we entered. The fallback check below must only read log lines
  # produced by the process this switch started — see the note there.
  date '+%Y-%m-%d %H:%M:%S' > "$STATE"
  log "GPU mode active; GPU now: $(gpumem)"
}

leave_gpu_mode() {
  log "LEAVE GPU mode — restoring CPU embeddings and chat"
  if [ -f "$ENV_BAK" ]; then cp -f "$ENV_BAK" "$ENVF"; else
    sed -i "s#^ORT_DYLIB_PATH=.*#ORT_DYLIB_PATH=$ORT_CPU#" "$ENVF"
    sed -i "s#^ECHO_EMBED_GPU=.*#ECHO_EMBED_GPU=0#" "$ENVF"
  fi
  rm -f "$DROPIN"
  systemctl daemon-reload
  systemctl restart noviq-server
  systemctl start noviq-vllm
  rm -f "$STATE" "$IDLE" "$BANNER"
  log "chat restored; GPU now: $(gpumem)"
}

# Emergency handle, now that leave_gpu_mode exists: paused means RELEASE if we
# are holding the GPU, then stop deciding.
if [ "$PAUSED" = 1 ]; then
  if [ -f "$STATE" ]; then
    log "paused ($PAUSE) while holding the GPU — releasing and restoring chat"
    leave_gpu_mode
  fi
  exit 0
fi

if [ ! -f "$STATE" ]; then
  # Idle -> busy. Trigger on a BATCH, not on loose pending files: the batch flag
  # is set after every file is on disk, so a restart cannot interrupt a transfer.
  if [ "$batches" -gt 0 ] && [ "$files" -gt 0 ]; then
    if [ "$batchsize" -lt "$MIN_BATCH" ]; then
      log "batch of $batchsize file(s) is below MIN_BATCH=$MIN_BATCH — staying on CPU"
    else
      enter_gpu_mode
    fi
  fi
  exit 0
fi

# ── Already in GPU mode: did the embedder actually land on the GPU? ──────────
# ⚠️ THIS CHECK HAD A FALSE POSITIVE (2026-08-14) that disabled auto GPU mode
# while the GPU was working perfectly. It grepped `--since "-10min"`, which swept
# in `Embedding model ready (gpu=false)` logged by the PREVIOUS, CPU-mode process
# four seconds before the switch. Meanwhile the new process had logged
# `Successfully registered CUDAExecutionProvider` and `(gpu=true)`.
#
# Two corrections: the window starts at the moment we entered GPU mode, and the
# SUCCESS signal is checked first. Absence of evidence is not failure — if
# neither line has appeared yet the embedder simply has not loaded (it loads on
# first use), so we wait rather than concluding anything.
SINCE=$(cat "$STATE" 2>/dev/null)
[ -z "$SINCE" ] && SINCE="-10min"
JL=$(journalctl -u noviq-server --since "$SINCE" --no-pager 2>/dev/null)
if grep -qiE 'Successfully registered .CUDAExecutionProvider|Embedding model ready \(gpu=true\)' <<<"$JL"; then
  : # confirmed on GPU
elif grep -qiE 'Embedding model ready \(gpu=false\)|not enabled in this build' <<<"$JL"; then
  log "CPU FALLBACK confirmed since $SINCE — chat is down for nothing; leaving"
  leave_gpu_mode
  touch "$PAUSE"
  log "auto GPU mode DISABLED ($PAUSE) — fix the CUDA runtime, then remove that file"
  exit 0
fi

# ⚠️ HOLD ON `files`, NOT on `batches`. A batch flag left open by a lost counter
# bump is permanent until something reconciles it, and this test used to read it
# as "work in progress" — so a stranded batch pinned GPU mode with chat down and
# zero files indexing, indefinitely (2026-08-14, 25/27). Pending/processing
# sources are the only signal that cannot strand: when there is no work, there is
# no work, whatever the flag says.
if [ "$files" -gt 0 ]; then
  echo 0 > "$IDLE"
  log "indexing (files=$files, batches=$batches) — holding GPU; $(gpumem)"
  exit 0
fi

n=$(( $(cat "$IDLE" 2>/dev/null || echo 0) + 1 ))
echo "$n" > "$IDLE"
log "quiet poll $n/$IDLE_POLLS"
[ "$n" -ge "$IDLE_POLLS" ] && leave_gpu_mode
exit 0
