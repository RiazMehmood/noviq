#!/usr/bin/env python3
"""
Stage 2+3 — Retrieve + teacher-answer generation (Feature 019).

Drives each question through the LIVE pipeline (`POST /api/query`), so the prod 8B is
the teacher AND retrieval is the real thing. Builds one SFT record per question from
the response `citations[]` (which carry the verbatim cited passages) + answer.

Usage:
  python generate_dataset.py \
      --base-url http://127.0.0.1:8080 \
      --username admin --password '...' \
      --questions questions.jsonl \
      --out raw.jsonl [--limit N] [--sleep 0.3]

`questions.jsonl` lines: {"question": "...", "source_q": "query_log|provision|teacher|adversarial"}
(a plain .txt with one question per line also works).

Run this ON or NEAR the VM (loopback URL) while the 8B/vLLM engine is active.
"""
import argparse
import sys
import time

import rag_sft_lib as lib


def load_questions(path):
    if path.endswith(".jsonl"):
        for rec in lib.read_jsonl(path):
            q = rec.get("question") or rec.get("text")
            if q:
                yield q.strip(), rec.get("source_q", "unknown")
    else:
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#"):
                    yield line, "unknown"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base-url", required=True)
    ap.add_argument("--username", required=True)
    ap.add_argument("--password", required=True)
    ap.add_argument("--questions", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--sleep", type=float, default=0.3, help="seconds between calls")
    args = ap.parse_args()

    print(f"logging in to {args.base_url} …", file=sys.stderr)
    token = lib.login(args.base_url, args.username, args.password)

    questions = list(load_questions(args.questions))
    if args.limit:
        questions = questions[: args.limit]
    print(f"{len(questions)} questions to process", file=sys.stderr)

    ok = 0
    fail = 0
    with open(args.out, "w", encoding="utf-8") as out:
        for i, (q, source_q) in enumerate(questions, 1):
            try:
                tr = lib.ask(args.base_url, token, q)
                rec = lib.to_record(q, tr, source_q=source_q)
                out.write(_dumps(rec) + "\n")
                out.flush()
                ok += 1
                kind = rec["meta"]["kind"]
                ncit = len(rec["meta"]["citations"])
                print(f"[{i}/{len(questions)}] {kind:7s} cites={ncit:2d}  {q[:60]}", file=sys.stderr)
            except lib.ApiError as e:
                fail += 1
                print(f"[{i}] FAIL {q[:50]} :: {e}", file=sys.stderr)
            time.sleep(args.sleep)

    print(f"\nDONE  ok={ok} fail={fail}  ->  {args.out}", file=sys.stderr)


def _dumps(rec):
    import json
    return json.dumps(rec, ensure_ascii=False)


if __name__ == "__main__":
    main()
