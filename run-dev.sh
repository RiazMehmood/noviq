#!/usr/bin/env bash
# Launch EchoAI Tauri dev server with the env this machine needs.
# Why: /home/riaz/.cargo is a broken NTFS-reparse symlink; CARGO_HOME must point
# at the real dir or any cargo subprocess fails with "File exists (os error 17)".
set -euo pipefail

cd "/media/riaz/New Volume/echo-AI"

export RUSTUP_TOOLCHAIN=stable
export CARGO_HOME="/media/riaz/New Volume/.cargo"
export CARGO_TARGET_DIR="/media/riaz/New Volume/echo-AI-target"
export PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH"

# Model storage on NTFS (avoids filling 41GB root /dev/sda5 with 2GB+ model).
# The app's download_model command and resolve_model_path both honour this var.
export ECHO_MODELS_DIR="/media/riaz/New Volume/echo-AI/models"
mkdir -p "$ECHO_MODELS_DIR"

# Pass-2 LLM entity-extraction cap (OOM fix from session 9).
# Note: Pass 2 itself is hardcoded OFF in indexer.rs as of 2026-05-25 to free
# CPU during indexing — this cap only matters if Pass 2 is re-enabled.
export ECHO_PASS2_CHUNK_LIMIT="${ECHO_PASS2_CHUNK_LIMIT:-10}"

# Structure-parser per-chunk char cap. Hardcoded default 500 in
# structure_parser.rs::long_chunk_threshold; env var here is a tuning knob
# for A/B testing without recompile. Lower = more chunks, finer recall.
export ECHO_LONG_CHUNK_THRESHOLD="${ECHO_LONG_CHUNK_THRESHOLD:-500}"

# Tesseract OCR tuning (parsers/ocr.rs). Defaults below override the in-code
# defaults (PSM=1, DPI=300) for the Sindh RoB 1986 re-OCR experiment: PSM=4
# treats each page as a single column of variable text, which avoids the
# column-interleaving that produced empty-shell (c)/(i) chunks under PSM 1.
# DPI 400 gives tesseract more pixels on the marginal-label text.
# Override per-launch: `ECHO_OCR_PSM=6 ./run-dev.sh` etc.
export ECHO_OCR_PSM="${ECHO_OCR_PSM:-4}"
export ECHO_OCR_DPI="${ECHO_OCR_DPI:-400}"

# Rust log level — METRIC lines need at least `info` for echo_ai crate.
export RUST_LOG="${RUST_LOG:-info,echo_ai=debug}"

# Full backtraces on panic — dev-only; no effect on release builds.
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

# ── ONNX Runtime for fastembed embedder + Tier 3 reranker ────────────────────
# `ort` is built with `load-dynamic`, so libonnxruntime.so must be discoverable
# at runtime. The `ort` crate (2.0.0-rc.9) and `fastembed` (4.9.1) are built
# against ONNX Runtime 1.20.x. Auto-globbing pip's site-packages picks up newer
# runtimes (e.g. 1.24.x under python3.14) whose ABI mismatch makes Session
# creation fail — "failed to initialise fastembed model", zero embeddings
# written, indexing silently aborts. So we pin to a vendored 1.20.1 runtime in
# .ort-runtime/; the pip glob remains only as a last-resort fallback.
if [ -z "${ORT_DYLIB_PATH:-}" ]; then
  VENDORED_ORT="/media/riaz/New Volume/echo-AI/.ort-runtime/libonnxruntime.so.1.20.1"
  if [ -f "$VENDORED_ORT" ]; then
    export ORT_DYLIB_PATH="$VENDORED_ORT"
    echo "[run-dev] ORT_DYLIB_PATH=$ORT_DYLIB_PATH (vendored 1.20.1 — matches ort crate)"
  else
    ORT_LIB="$(ls /home/riaz/.local/lib/python*/site-packages/onnxruntime/capi/libonnxruntime.so.* 2>/dev/null | head -1)"
    if [ -n "$ORT_LIB" ]; then
      export ORT_DYLIB_PATH="$ORT_LIB"
      echo "[run-dev] WARN: vendored 1.20.1 runtime missing; falling back to $ORT_LIB (ABI may mismatch ort crate)" >&2
    else
      echo "[run-dev] WARN: onnxruntime .so not found; embedder + reranker will fail" >&2
    fi
  fi
fi

# Sanity: refuse to start if CARGO_HOME isn't a usable directory.
if [ ! -d "$CARGO_HOME" ]; then
  echo "ERROR: CARGO_HOME=$CARGO_HOME is not a directory" >&2
  exit 1
fi

# ── Structured log file ───────────────────────────────────────────────────────
# All stdout+stderr is tee'd to a timestamped log file so monitor-dev.sh can
# parse METRIC lines and tail recent output without needing a Claude task log.
LOG_DIR="/media/riaz/New Volume/echo-AI/logs"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/dev-$(date +%Y%m%d-%H%M%S).log"
# Write sentinel so monitor-dev.sh auto-discovers the current log.
echo "$LOG_FILE" > /tmp/echoai-dev-log
echo "[run-dev] Logging to: $LOG_FILE"
echo "[run-dev] Monitor:    ./monitor-dev.sh"

# ── Launch ───────────────────────────────────────────────────────────────────
# Wrap in a dedicated systemd scope so systemd-oomd cannot kill this process
# as collateral damage when the user-slice memory pressure > 50% threshold is
# breached. The LLM (1.9 GB) + ONNX embedder loaded simultaneously during
# indexing of large PDFs pushes that threshold; without this wrapper the entire
# GNOME Terminal cgroup (121 processes) gets killed by systemd-oomd.
RUN_CMD="npm run tauri dev 2>&1 | tee \"$LOG_FILE\""
if command -v systemd-run &>/dev/null; then
  # --collect ensures the scope unit is garbage-collected as soon as the process
  # exits, so a re-run never hits "Unit echoai-dev.scope already exists".
  exec systemd-run --user --scope --collect \
    --unit=echoai-dev \
    -p ManagedOOMMemoryPressureLimit=100% \
    -- bash -c "$RUN_CMD"
else
  eval "$RUN_CMD"
fi
