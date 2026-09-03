#!/usr/bin/env bash
# monitor-dev.sh — Compact EchoAI live dashboard
# Usage: ./monitor-dev.sh [path/to/dev.log]
# Env:   INTERVAL=4  (refresh seconds)

set +ue
INTERVAL="${INTERVAL:-4}"
LOG_DIR="/media/riaz/New Volume/echo-AI/logs"

RED=$'\033[1;31m'; YEL=$'\033[1;33m'; GRN=$'\033[1;32m'
BLU=$'\033[1;34m'; CYA=$'\033[1;36m'; DIM=$'\033[2m'; RST=$'\033[0m'
CLR=$'\033[2J\033[H'

cleanup() { printf '\n%sMonitor stopped.%s\n' "$DIM" "$RST"; exit 0; }
trap cleanup INT TERM

draw() {
  # ── Auto-find latest dev log ───────────────────────────────────────────────
  if [ -n "$1" ] && [ -f "$1" ]; then
    LOG="$1"
  else
    LOG=$(ls -t "$LOG_DIR"/dev-*.log 2>/dev/null | head -1)
  fi

  TS=$(date '+%H:%M:%S')

  # ── System ────────────────────────────────────────────────────────────────
  while IFS=': ' read -r k v _; do
    case "$k" in
      MemTotal)     mt=$v ;;
      MemAvailable) ma=$v ;;
    esac
  done < /proc/meminfo
  mu=$(( mt - ma ))
  mp=$(( 100 * mu / mt ))
  load=$(awk '{print $1}' /proc/loadavg)
  [ "$mp" -ge 90 ] && mc="$RED" || { [ "$mp" -ge 75 ] && mc="$YEL" || mc="$GRN"; }

  # ── App process ───────────────────────────────────────────────────────────
  APID=$(pgrep -f "echo-AI-target.*/echo-ai$|echo-ai-target.*/echo-ai$" 2>/dev/null | head -1)
  if [ -n "$APID" ]; then
    read -r pcpu pmem prss _ <<< "$(ps -p "$APID" -o %cpu,%mem,rss --no-headers 2>/dev/null)"
    rss_mb=$(( ${prss:-0} / 1024 ))
    pline="${GRN}pid=$APID  rss=${rss_mb}MB  cpu=${pcpu}%${RST}"
  else
    pline="${RED}not running — start with ./run-dev.sh${RST}"
  fi

  # ── LLM status ────────────────────────────────────────────────────────────
  if [ -n "$LOG" ]; then
    llm_path=$(grep 'Loaded LLM:' "$LOG" 2>/dev/null | tail -1 | grep -oP 'Loaded LLM: \K.+')
    llm_evictions=$(grep -c 'LLM evicted' "$LOG" 2>/dev/null || echo 0)
    llm_fails=$(grep -c 'LLM inference failed\|try_llm_infer.*Err\|llm.*error' "$LOG" 2>/dev/null || echo 0)
    if [ -n "$llm_path" ]; then
      llm_name=$(basename "$llm_path")
      llm_status="${GRN}● loaded${RST}  $llm_name"
    elif [ -n "$APID" ]; then
      llm_status="${YEL}○ not loaded  (no query sent yet in this session)${RST}"
    else
      llm_status="${DIM}○ app not running${RST}"
    fi
  else
    llm_status="${DIM}○ no log found${RST}"
    llm_evictions=0; llm_fails=0
  fi

  # ── Query table ───────────────────────────────────────────────────────────
  if [ -n "$LOG" ]; then
    q_total=$(grep -c 'METRIC query_total_ms' "$LOG" 2>/dev/null || echo 0)
    avg_total_s=$(grep 'METRIC query_total_ms' "$LOG" 2>/dev/null \
      | grep -oP 'query_total_ms=\K[0-9]+' \
      | awk '{s+=$1;c++} END{if(c>0)printf "%ds",int(s/c/1000);else print "-"}')
    avg_llm_s=$(grep 'METRIC llm_done_ms' "$LOG" 2>/dev/null \
      | grep -oP 'llm_done_ms=\K[0-9]+' \
      | awk '{s+=$1;c++} END{if(c>0)printf "%ds",int(s/c/1000);else print "-"}')

    # Build last-5 table: pair metrics per query using awk
    query_rows=$(grep -E 'METRIC (query_start|embed_query_ms|llm_start|llm_done_ms|query_total_ms)' \
        "$LOG" 2>/dev/null \
      | awk '
        /METRIC query_start/   { split($0,a," "); ts=substr(a[1],2,8)
                                  embed="-"; llm_s="-"; tot_s="-"; prompt="-"; ans="-"; cites="-" }
        /METRIC embed_query_ms/{ match($0,/embed_query_ms=([0-9]+)/,m); embed=m[1]"ms" }
        /METRIC llm_start/     { match($0,/prompt_chars=([0-9]+)/,m); prompt=m[1]"c" }
        /METRIC llm_done_ms/   { match($0,/llm_done_ms=([0-9]+)/,m);   llm_s=int(m[1]/1000)"s"
                                  match($0,/answer_chars=([0-9]+)/,m); ans=m[1]"c" }
        /METRIC query_total_ms/{ match($0,/query_total_ms=([0-9]+)/,m); tot_s=int(m[1]/1000)"s"
                                  match($0,/citations=([0-9]+)/,m);     cites=m[1]
                                  print ts"|"tot_s"|"llm_s"|"embed"|"prompt"|"ans"|"cites }
      ' | tail -5)

    # In-progress detection
    last_q_ts=$(grep 'METRIC query_start'    "$LOG" 2>/dev/null | tail -1 | grep -oP '\d{2}:\d{2}:\d{2}')
    last_d_ts=$(grep 'METRIC query_total_ms' "$LOG" 2>/dev/null | tail -1 | grep -oP '\d{2}:\d{2}:\d{2}')
    [ "$last_q_ts" != "$last_d_ts" ] && in_progress="$last_q_ts" || in_progress=""
    last_embed=$(grep 'METRIC embed_query_ms' "$LOG" 2>/dev/null | tail -1 | grep -oP 'embed_query_ms=\K[0-9]+')
    last_prompt=$(grep 'METRIC llm_start'     "$LOG" 2>/dev/null | tail -1 | grep -oP 'prompt_chars=\K[0-9]+')
  else
    q_total=0; avg_total_s="-"; avg_llm_s="-"; query_rows=""; in_progress=""
  fi

  # ── Errors ────────────────────────────────────────────────────────────────
  if [ -n "$LOG" ]; then
    errs=$(grep -iE ' (ERROR|panic) ' "$LOG" 2>/dev/null \
      | grep -v 'RUST_LOG\|monitor-dev\|n_ctx_seq.*<.*n_ctx_train' | tail -3)
  else
    errs=""
  fi

  # ══ DRAW ══════════════════════════════════════════════════════════════════
  printf '%s' "$CLR"
  printf '%s╔══ EchoAI  %s ══════════════════════════════════╗%s\n' "$CYA" "$TS" "$RST"

  # System + Process (2 lines)
  printf '%s SYS%s   RAM %s%dMB/%dMB(%d%%)%s  load=%-4s\n' \
    "$BLU" "$RST" "$mc" "$((mu/1024))" "$((mt/1024))" "$mp" "$RST" "$load"
  printf '%s PROC%s  %b\n' "$BLU" "$RST" "$pline"

  # LLM status (2 lines)
  printf '\n%s LLM%s   %b\n' "$BLU" "$RST" "$llm_status"
  printf '        evictions=%-2s  inference_errors=%s\n' "$llm_evictions" "$llm_fails"

  # Query table
  printf '\n%s QUERIES%s  total=%-3s  avg_total=%-6s avg_llm=%s\n' \
    "$BLU" "$RST" "$q_total" "$avg_total_s" "$avg_llm_s"
  if [ "$q_total" -eq 0 ]; then
    printf '  %s(no queries yet)%s\n' "$DIM" "$RST"
  else
    printf '  %s%-8s %-7s %-7s %-8s %-8s %-6s %s%s\n' \
      "$DIM" "Time" "Total" "LLM" "Embed" "Prompt" "Answer" "Cites" "$RST"
    while IFS='|' read -r ts tot_s llm_s embed prompt ans cites; do
      [ -z "$ts" ] && continue
      t_num=${tot_s%s}
      if   [ "${t_num:-999}" -le 45 ]; then dot="${GRN}●${RST}"
      elif [ "${t_num:-999}" -le 90 ]; then dot="${YEL}●${RST}"
      else                                   dot="${RED}●${RST}"; fi
      printf '  %b %-8s %-7s %-7s %-8s %-8s %-6s %s\n' \
        "$dot" "$ts" "$tot_s" "$llm_s" "$embed" "$prompt" "$ans" "$cites"
    done <<< "$query_rows"
  fi

  # In-progress indicator
  if [ -n "$in_progress" ]; then
    printf '  %s⟳ running since %s' "$YEL" "$in_progress"
    [ -n "$last_embed" ] && printf '  embed=%sms' "$last_embed"
    [ -n "$last_prompt" ] && printf '  prompt=%sc' "$last_prompt"
    printf '  ...%s\n' "$RST"
  fi

  # Errors
  printf '\n%s ERRORS%s  ' "$BLU" "$RST"
  if [ -z "$errs" ]; then
    printf '%s✓ clean%s\n' "$GRN" "$RST"
  else
    printf '\n'
    while IFS= read -r line; do
      short=$(printf '%s' "$line" | cut -c1-100)
      printf '  %s%s%s\n' "$RED" "$short" "$RST"
    done <<< "$errs"
  fi

  # Footer
  log_short=$(basename "${LOG:-(none)}")
  printf '\n%s refresh=%ss  log=%s  Ctrl-C to exit%s\n' "$DIM" "$INTERVAL" "$log_short" "$RST"
}

while :; do
  draw "$1" 2>/dev/null || true
  sleep "$INTERVAL"
done
