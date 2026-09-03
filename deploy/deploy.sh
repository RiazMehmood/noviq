#!/usr/bin/env bash
# F014 (T029) — build & redeploy noviq-server from origin/release.
#
# Deploy model (restored 2026-06-27): the server tracks the stable `release`
# branch; you develop on feature branches and `make deploy` promotes the current
# branch -> origin/release. So the VM is never reconfigured when a new feature
# branch starts. See VM_HANDOFF_VERIFY_DEPLOY.md.
#
# NOTE: this in-tree copy is REFERENCE only. The live VM runs an out-of-tree copy
# at /opt/noviq/deploy.sh (verified 2026-06-27). BRANCH below mirrors that
# operational script so the two never disagree; override via `BRANCH=` env.
#
# Runs as the DECOUPLED noviq-deploy.service (oneshot), NOT as a child of
# noviq-server — so the `systemctl restart` below cannot SIGKILL this build
# mid-flight (FR-015).
#
# User-visible "updating" banner: this script touches a DEPLOYING sentinel under
# the server's data dir. While it exists, the still-running old binary reports
# `{status:"deploying"}` from /api/health and the browser shows a banner. The
# sentinel is cleared before the restart (so the new binary probes `ok`) and on
# any exit (so the banner never sticks). See deployment-and-updates.md.
#
# Auto-rolls back to the previous binary + commit on a failed health probe
# (FR-013/FR-017).
set -euo pipefail

# Vars are env-overridable so deploy/test_deploy_rollback.sh (T035) can exercise
# the build→probe→rollback logic against a throwaway repo with stubbed tools.
REPO="${REPO:-/opt/noviq/echoAI}"
BRANCH="${BRANCH:-release}"
SVC="${SVC:-noviq-server}"
BIN="${BIN:-$REPO/src-tauri/target/release/noviq-server}"
LOG="${LOG:-/var/log/noviq/deploy.log}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:8080/api/health}"

mkdir -p "$(dirname "$LOG")"
exec >>"$LOG" 2>&1

cd "$REPO"

# Resolve the data dir the running server uses, so the sentinel lands where
# /api/health looks (NOVIQ_DATA_DIR in .env; default ./.noviq-data).
NOVIQ_DATA_DIR="$( . ./.env >/dev/null 2>&1; echo "${NOVIQ_DATA_DIR:-$REPO/.noviq-data}" )"
SENTINEL="$NOVIQ_DATA_DIR/DEPLOYING"
# Circuit-breaker + trigger state shared with check-release.sh (2026-07-24):
#   FAILED_SHA — a commit whose deploy rolled back; never auto-retried.
#   LAST_SHA   — last sha we ATTEMPTED, so a rolled-back HEAD can't re-arm the poll.
FAILED_SHA_FILE="${FAILED_SHA_FILE:-$NOVIQ_DATA_DIR/DEPLOY_FAILED_SHA}"
LAST_SHA_FILE="${LAST_SHA_FILE:-$NOVIQ_DATA_DIR/LAST_DEPLOYED_SHA}"
mkdir -p "$NOVIQ_DATA_DIR"

echo "=== deploy $(date -Is) ==="

# Always clear the "deploying" marker on exit (success, failure, or rollback).
cleanup() { rm -f "$SENTINEL"; }
trap cleanup EXIT

PREV_COMMIT=$(git rev-parse HEAD)
[ -f "$BIN" ] && cp -f "$BIN" "$BIN.prev" || true

# Banner ON: old server now reports status:"deploying" for the whole build.
date -Is > "$SENTINEL"

git fetch --prune origin
TARGET_SHA=$(git rev-parse "origin/$BRANCH")
# Record the ATTEMPT before building. check-release.sh compares against this instead of
# the working-tree HEAD, so a rollback (which moves HEAD backwards) cannot make the poll
# think there is new work every 2 minutes — the loop this file used to sustain.
echo "$TARGET_SHA" > "$LAST_SHA_FILE"
git checkout "$BRANCH"
git reset --hard "origin/$BRANCH"

# Frontend → dist/ (served as static by the binary).
npm ci
npm run build

# Backend (CUDA). Additive migrations apply on server startup.
# --no-default-features drops the desktop (tauri/webkit) feature — absent on the
# headless GPU box. CUDA host objects must be PIC (nvcc -Xcompiler -fPIC) or the
# final link fails with R_X86_64_PC32 against libc symbols. Arch 86 = A40 (Ampere).
# target-cpu=native → llama-cpp-sys sets GGML_NATIVE=ON so ggml-cpu matches THIS
# host's instruction set; the IBA VM's vCPU is masked to a pre-AVX Core2 and the
# default AVX2 ggml build SIGILLs on it. Build host MUST equal the run host.
( cd src-tauri \
  && CMAKE_CUDA_ARCHITECTURES="${CMAKE_CUDA_ARCHITECTURES:-86}" \
     CUDAFLAGS="${CUDAFLAGS:--Xcompiler -fPIC}" \
     RUSTFLAGS="${RUSTFLAGS:--C target-cpu=native}" \
     cargo build --release --no-default-features --features "llm,server,cuda" --bin noviq-server )

# Clear the marker BEFORE restart so the freshly-started binary reports
# status:"ok" to the deploy health probe (FR-017).
rm -f "$SENTINEL"

# F016 (T025) — when NOVIQ_INFER_ENGINE=vllm, make sure the loopback inference
# sidecar is up before the server restarts onto it. This NEVER rebuilds vLLM (it
# is provisioned out-of-tree by setup-vllm.sh and survives git reset) — it only
# nudges systemd to (re)start the unit if it isn't already active. A llama.cpp box
# has the var unset/`llamacpp`, so this is a no-op there.
INFER_ENGINE="$( . ./.env >/dev/null 2>&1; echo "${NOVIQ_INFER_ENGINE:-llamacpp}" )"
if [ "$INFER_ENGINE" = "vllm" ]; then
  if ! systemctl is-active --quiet noviq-vllm; then
    echo "ensuring noviq-vllm sidecar is active (engine=vllm)"
    sudo systemctl start noviq-vllm || echo "WARN: could not start noviq-vllm (server will degrade per FR-011)"
  fi
fi

sudo systemctl restart "$SVC"

# Post-restart health probe; roll back on failure.
for _ in $(seq 1 30); do
  if curl -fsS "$HEALTH_URL" >/dev/null; then
    echo "deploy OK -> $(git rev-parse --short HEAD)"
    # Clear any parked failure: this sha is good, so the breaker must not stay tripped.
    rm -f "$FAILED_SHA_FILE"
    exit 0
  fi
  sleep 2
done

echo "HEALTH CHECK FAILED — rolling back to $PREV_COMMIT"
# Trip the circuit breaker BEFORE rolling back. Without this, the rollback leaves HEAD
# behind origin/release, the 2-minute poll sees a difference, and the SAME broken commit
# is rebuilt (full CUDA build) every cycle, forever. Cleared automatically by the next
# successful deploy, or by hand: rm this file to force a retry of the same sha.
echo "$TARGET_SHA" > "$FAILED_SHA_FILE"
echo "circuit breaker tripped for $TARGET_SHA — auto-deploy of this sha is now blocked"
git reset --hard "$PREV_COMMIT"
[ -f "$BIN.prev" ] && cp -f "$BIN.prev" "$BIN"
sudo systemctl restart "$SVC"
exit 1
