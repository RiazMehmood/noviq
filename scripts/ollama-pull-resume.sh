#!/usr/bin/env bash
# Resilient `ollama pull` — retries on network failure.
# Ollama caches blobs in ~/.ollama/models, so each retry resumes
# from the last completed layer instead of starting over.
#
# Usage: ./ollama-pull-resume.sh qwen2.5:1.5b

set -u

MODEL="${1:-qwen2.5:1.5b}"
MAX_ATTEMPTS="${MAX_ATTEMPTS:-100}"
SLEEP_BETWEEN="${SLEEP_BETWEEN:-5}"

# Already present? exit immediately.
if ollama list 2>/dev/null | awk 'NR>1 {print $1}' | grep -qx "$MODEL"; then
  echo "[ok] $MODEL already pulled — nothing to do."
  exit 0
fi

attempt=1
while (( attempt <= MAX_ATTEMPTS )); do
  echo "[attempt $attempt/$MAX_ATTEMPTS] ollama pull $MODEL"
  if ollama pull "$MODEL"; then
    echo "[done] $MODEL pulled successfully."
    exit 0
  fi

  # Re-check in case the pull actually finished but exit code was noisy.
  if ollama list 2>/dev/null | awk 'NR>1 {print $1}' | grep -qx "$MODEL"; then
    echo "[done] $MODEL is present after attempt $attempt."
    exit 0
  fi

  echo "[warn] pull failed; retrying in ${SLEEP_BETWEEN}s..."
  sleep "$SLEEP_BETWEEN"
  attempt=$((attempt + 1))
done

echo "[fail] gave up after $MAX_ATTEMPTS attempts."
exit 1
