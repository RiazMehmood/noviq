#!/usr/bin/env bash
# Offline LLM smoke test (Ollama edition) — replay the exact `Full prompt sent
# to LLM` block from one or more EchoAI query-trace files through Ollama's
# /api/generate endpoint, on both the fine-tuned (noviq) and base (Qwen2.5-3B)
# models. Distinguishes "the LLM itself is the bug" from "the app prompt
# assembly is the bug" on the Q5/Q9 failures observed in the 2026-06-05 eval.
#
# Setup:
#   ollama create qwen3b-base -f /tmp/qwen3b-base.Modelfile
#   ollama create noviq -f /tmp/noviq.Modelfile        # when noviq.gguf is local
#
# Usage:
#   ./tools/smoke_test_llm.sh trace1.log [trace2.log ...]
#
# Env overrides:
#   BASE_MODEL  (default: qwen3b-base)
#   NOVIQ_MODEL (default: noviq) — set to "" to skip the noviq run
#   OLLAMA_URL  (default: http://localhost:11434)

set -euo pipefail

OLLAMA_URL="${OLLAMA_URL:-http://localhost:11434}"
BASE_MODEL="${BASE_MODEL:-qwen3b-base}"
NOVIQ_MODEL="${NOVIQ_MODEL:-noviq}"
OUT_DIR="${OUT_DIR:-/tmp/echoai-smoke-$(date +%s)}"

mkdir -p "$OUT_DIR"

curl -sf "$OLLAMA_URL/api/tags" >/dev/null || {
    echo "✗ Ollama not reachable at $OLLAMA_URL"
    exit 1
}

# Confirm models are present.
have_model() {
    curl -s "$OLLAMA_URL/api/tags" | grep -q "\"name\":\"$1"
}
if ! have_model "$BASE_MODEL"; then
    echo "✗ Base model '$BASE_MODEL' not in Ollama. ollama list:"; ollama list; exit 1
fi
NOVIQ_AVAILABLE=0
if [[ -n "$NOVIQ_MODEL" ]] && have_model "$NOVIQ_MODEL"; then
    NOVIQ_AVAILABLE=1
fi

extract_prompt() {
    local trace="$1" out="$2"
    awk '
      /Full prompt sent to LLM/ {capture=1; next}
      capture && /^    └──/ {capture=0}
      capture && /\[…truncated;/ {next}
      capture && /^    │/ {
        sub(/^    │ ?/, "")
        print
      }
    ' "$trace" > "$out"
}

extract_question() {
    grep -m1 "Question:" "$1" | sed 's/.*Question: *//' | sed 's/ *$//'
}

run_ollama() {
    local model="$1" prompt_file="$2"
    # raw=true so Ollama bypasses its template and sends the prompt as-is.
    python3 - "$model" "$prompt_file" "$OLLAMA_URL" <<'PY'
import json, sys, urllib.request
model, prompt_file, url = sys.argv[1], sys.argv[2], sys.argv[3]
with open(prompt_file) as f: prompt = f.read()
payload = {
    "model": model, "prompt": prompt, "stream": False, "raw": True,
    "options": {"temperature": 0, "top_k": 1, "repeat_penalty": 1.3,
                "repeat_last_n": 256, "num_ctx": 3200, "num_predict": 256,
                "stop": ["<|im_end|>", "<|im_start|>"]},
}
req = urllib.request.Request(
    f"{url}/api/generate",
    data=json.dumps(payload).encode(),
    headers={"Content-Type": "application/json"},
)
with urllib.request.urlopen(req, timeout=600) as r:
    data = json.loads(r.read().decode())
print(data.get("response") or data.get("error") or "(empty)")
PY
}

[[ $# -lt 1 ]] && { echo "usage: $0 <trace.log> [<trace.log> ...]"; exit 2; }

echo "═══════════════════════════════════════════════════════════════════════"
echo " EchoAI offline LLM smoke test (Ollama)"
echo "═══════════════════════════════════════════════════════════════════════"
echo " Ollama       : $OLLAMA_URL"
echo " Base model   : $BASE_MODEL"
echo " Noviq model  : ${NOVIQ_MODEL:-(disabled)} [$([ "$NOVIQ_AVAILABLE" = "1" ] && echo present || echo absent)]"
echo " Output dir   : $OUT_DIR"
echo

for trace in "$@"; do
    qbase="$(basename "$trace" .log)"
    pfile="$OUT_DIR/${qbase}.prompt.txt"
    extract_prompt "$trace" "$pfile"
    bytes=$(wc -c < "$pfile")
    [[ "$bytes" -lt 100 ]] && { echo "✗ no prompt extracted from $trace"; continue; }
    question="$(extract_question "$trace")"
    echo "──────────────────────────────────────────────────────────────────────"
    echo " Trace: $(basename "$trace")"
    echo " Q    : $question"
    echo " Prompt bytes: $bytes"
    echo "──────────────────────────────────────────────────────────────────────"

    if [[ "$NOVIQ_AVAILABLE" = "1" ]]; then
        echo "── NOVIQ ($NOVIQ_MODEL) ──"
        a="$(run_ollama "$NOVIQ_MODEL" "$pfile")"
        echo "$a" | tee "$OUT_DIR/${qbase}.noviq.txt"
        echo
    fi
    echo "── BASE  ($BASE_MODEL) ──"
    b="$(run_ollama "$BASE_MODEL" "$pfile")"
    echo "$b" | tee "$OUT_DIR/${qbase}.base.txt"
    echo
done

echo "═══════════════════════════════════════════════════════════════════════"
echo " Outputs saved to: $OUT_DIR"
echo "═══════════════════════════════════════════════════════════════════════"
