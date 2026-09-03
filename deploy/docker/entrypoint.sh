#!/usr/bin/env bash
# Noviq container entrypoint.
#
# Three jobs, all of which exist because of something specific in the codebase:
#   1. Park the fastembed model cache on the volume (1.1 GB — must not re-download).
#   2. Point ORT at the onnxruntime we shipped, if we shipped one.
#   3. Drop to a non-root user, because PostgreSQL refuses to start as root.
set -euo pipefail

DATA="${NOVIQ_DATA_DIR:-/data}"
mkdir -p "$DATA"

# services/embedder.rs `cache_dir()` walks UP from the executable looking for an
# existing `.fastembed_cache` directory (there is no env override). The exe is at
# /opt/noviq/bin/, so a symlink one level up is what it finds — and it resolves
# onto the volume, so the nomic-embed download survives `docker rm`.
mkdir -p "$DATA/fastembed_cache"
ln -sfn "$DATA/fastembed_cache" /opt/noviq/.fastembed_cache

# fastembed/ort dlopen's onnxruntime. If the builder stage produced a .so we
# copied it into /opt/noviq/lib; point ORT at it unless the operator overrode it.
if [ -z "${ORT_DYLIB_PATH:-}" ]; then
  ort_lib="$(ls /opt/noviq/lib/libonnxruntime.so* 2>/dev/null | head -1 || true)"
  if [ -n "$ort_lib" ]; then
    export ORT_DYLIB_PATH="$ort_lib"
  fi
fi

if [ "$(id -u)" = "0" ]; then
  # initdb aborts as root, and the volume is root-owned on first mount.
  chown -R noviq:noviq "$DATA"
  chown -R noviq:noviq /opt/noviq
  exec gosu noviq "$@"
fi

exec "$@"
