#!/usr/bin/env bash
# F016 (T016) — concurrent load test for /api/query (SC-001..004).
#
# Fires N concurrent authenticated chat turns at the server, records per-request
# end-to-end latency + HTTP status, samples GPU memory/util during the run, and
# prints p50/p95, throughput, and failure rate. Engine-agnostic: run it against
# the llama.cpp baseline first (NOVIQ_INFER_ENGINE=llamacpp) and again against
# vLLM to prove SC-003 (>= 2.5x throughput).
#
# Usage:   bash load_test.sh [N]          # N concurrency, default 10 (try 10, 20, 50)
# Env:     BASE_URL (http://127.0.0.1:8080)  USER/PASS (admin creds)
#          QUESTION (the prompt)   PASS_P95_MS (10000 — SC-002 gate)
set -uo pipefail

N="${1:-10}"
BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
USER="${USER:-admin}"
PASS="${PASS:?set PASS=<admin password>}"
QUESTION="${QUESTION:-What are the approval limits of the DDWP?}"
PASS_P95_MS="${PASS_P95_MS:-10000}"
OUT="$(mktemp -d)"; trap 'rm -rf "$OUT"' EXIT

echo "=== login ==="
TOKEN="$(curl -fsS "$BASE_URL/api/login" -H 'Content-Type: application/json' \
  -d "{\"username\":\"$USER\",\"password\":\"$PASS\"}" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')"
[ -n "$TOKEN" ] || { echo "login failed"; exit 1; }

# GPU sampler (background) — peak mem is the SC-004 (no-OOM) signal.
( for _ in $(seq 1 120); do
    nvidia-smi --query-gpu=memory.used,memory.total,utilization.gpu \
      --format=csv,noheader,nounits 2>/dev/null >> "$OUT/gpu.csv"
    sleep 1
  done ) &
GPU_PID=$!

echo "=== firing $N concurrent /api/query ==="
one() {
  local i="$1"
  local code t body
  # Capture the BODY too: the FR-011 "service temporarily unavailable" message is
  # returned as HTTP 200 with an error answer, so http-code alone would count an
  # engine outage as a fast "success" and report bogus low latency. Treat the
  # unavailable marker (or an empty/missing answer) as a failure by emitting 503.
  body="$(curl -s -w '\n%{http_code} %{time_total}' \
    "$BASE_URL/api/query" \
    -H 'Content-Type: application/json' \
    -H "Authorization: Bearer $TOKEN" \
    -d "{\"conversationId\":\"load-$i-$RANDOM\",\"text\":\"$QUESTION\"}")"
  read -r code t <<<"$(printf '%s' "$body" | tail -n1)"
  local ans; ans="$(printf '%s' "$body" | sed '$d')"
  if printf '%s' "$ans" | grep -qiE 'temporarily unavailable|model unavailable' \
     || ! printf '%s' "$ans" | grep -q '"answer"'; then
    code=503   # engine outage / empty — count as failure, not a fast success
  fi
  echo "$code $t" >> "$OUT/res.csv"
}
export -f one; export BASE_URL TOKEN QUESTION OUT

WALL_START="$(date +%s.%N)"
seq 1 "$N" | xargs -P "$N" -I{} bash -c 'one "$@"' _ {}
WALL_END="$(date +%s.%N)"
kill "$GPU_PID" 2>/dev/null || true

echo "=== results ==="
awk -v wall="$(echo "$WALL_END - $WALL_START" | bc)" -v gate="$PASS_P95_MS" '
  { code=$1; ms=$2*1000; if (code==200){ ok++; a[ok]=ms } else { fail++ } }
  END {
    n=ok; if(n==0){ print "ALL REQUESTS FAILED ("fail" errors)"; exit 1 }
    # sort ascending
    for(i=1;i<=n;i++) for(j=i+1;j<=n;j++) if(a[j]<a[i]){t=a[i];a[i]=a[j];a[j]=t}
    p50=a[int(0.50*n+0.999)]; p95=a[int(0.95*n+0.999)]
    printf "served   : %d/%d (failures: %d)\n", ok, ok+fail, fail+0
    printf "p50      : %.0f ms\n", p50
    printf "p95      : %.0f ms  (gate %d ms -> %s)\n", p95, gate, (p95<=gate?"PASS":"FAIL")
    printf "min/max  : %.0f / %.0f ms\n", a[1], a[n]
    printf "wall     : %.1f s  ->  throughput %.1f req/min\n", wall, (ok/wall)*60
  }' "$OUT/res.csv"

if [ -s "$OUT/gpu.csv" ]; then
  echo "=== GPU during run (used/total MiB, util%) ==="
  sort -t, -k1 -n "$OUT/gpu.csv" | tail -1 | awk -F, '{printf "peak mem: %s / %s MiB   (SC-004: must stay < total, no OOM)\n",$1,$2}'
fi
