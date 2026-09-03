"""LLM-only re-run against Groq API (Llama-3.3-70B), reusing cached retrieval.

Same retrieval and reranker output as `nomic_full`; only the final LLM call
is swapped from local Qwen2.5-0.5B GGUF to Groq's hosted Llama-70B.

API key: read from /home/riaz/Desktop/groq_api_key.odt (ODT zipped XML).
"""
from __future__ import annotations

import argparse
import json
import re
import sqlite3
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
DB_PATH = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
KEY_FILE = Path("/home/riaz/Desktop/groq_api_key.odt")
GROQ_URL = "https://api.groq.com/openai/v1/chat/completions"

# Reuse prompt variants from the local-LLM run
sys.path.insert(0, str(ROOT / "scripts/reranker_eval"))
from rerun_llm import PROMPTS, build_context_string, gold_match


def read_key() -> str:
    with zipfile.ZipFile(KEY_FILE) as zf:
        content = zf.read("content.xml").decode("utf-8", errors="ignore")
    m = re.search(r"gsk_[A-Za-z0-9]{20,}", content)
    if not m:
        raise SystemExit("could not find gsk_ token in odt")
    return m.group(0)


def fetch_chunks(con: sqlite3.Connection, ids: list[str]) -> list[dict]:
    if not ids:
        return []
    placeholders = ",".join("?" * len(ids))
    rows = con.execute(
        f"""
        SELECT c.id, c.content, c.page_number,
               COALESCE(s.file_name, s.file_path, 'unknown') AS file_name
        FROM chunks c JOIN sources s ON s.id = c.source_id
        WHERE c.id IN ({placeholders})
        """,
        ids,
    ).fetchall()
    by_id = {r["id"]: dict(r) for r in rows}
    return [by_id[i] for i in ids if i in by_id]


def call_groq(api_key: str, model: str, system: str, user: str, max_tokens: int = 1024) -> dict:
    body = json.dumps({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "temperature": 0.0,
        "max_tokens": max_tokens,
    }).encode("utf-8")
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
        "User-Agent": "curl/8.0.1",
    }
    t0 = time.time()
    for attempt in range(6):
        req = urllib.request.Request(GROQ_URL, data=body, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                data = json.loads(resp.read().decode("utf-8"))
            break
        except urllib.error.HTTPError as e:
            if e.code == 429:
                retry_after = e.headers.get("retry-after") or e.headers.get("Retry-After")
                wait = float(retry_after) if retry_after else 30.0
                wait = min(max(wait, 5.0), 120.0)
                print(f"    [429] waiting {wait:.0f}s (attempt {attempt+1}/6)", flush=True)
                time.sleep(wait)
                continue
            raise
    else:
        raise RuntimeError("rate-limited after 6 retries")
    elapsed_ms = int((time.time() - t0) * 1000)
    return {
        "answer": data["choices"][0]["message"]["content"],
        "answer_chars": len(data["choices"][0]["message"]["content"]),
        "prompt_tokens": data["usage"]["prompt_tokens"],
        "completion_tokens": data["usage"]["completion_tokens"],
        "elapsed_ms": elapsed_ms,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True)
    ap.add_argument("--prompt", default="v3_few_shot", choices=list(PROMPTS.keys()))
    ap.add_argument("--model", default="llama-3.3-70b-versatile")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    api_key = read_key()
    print(f"[init] Groq key loaded, model={args.model}, prompt={args.prompt}", flush=True)

    src = Path(args.source)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    system_prompt = PROMPTS[args.prompt]

    con = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row

    trace_files = sorted(src.glob("q*.trace.json"))
    if not trace_files:
        sys.exit(f"no traces in {src}")

    # Skip any qN where an output trace already has non-empty answers on both branches
    def _done(p: Path) -> bool:
        if not p.exists():
            return False
        try:
            d = json.loads(p.read_text())
            return all(d["stages"][b].get("answer") for b in ("branch_a", "branch_b"))
        except Exception:
            return False
    before = len(trace_files)
    trace_files = [tf for tf in trace_files if not _done(out / tf.name)]
    print(f"[skip] {before - len(trace_files)} already complete; processing {len(trace_files)}", flush=True)

    manifest = {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime()),
        "source_run": str(src),
        "prompt_variant": args.prompt,
        "llm_provider": "groq",
        "llm_model": args.model,
        "note": "LLM-only re-run against Groq API; retrieval/rerank reused from source_run",
    }
    (out / "run_manifest.json").write_text(json.dumps(manifest, indent=2))

    total_in = total_out = 0
    for tf in trace_files:
        t = json.loads(tf.read_text())
        qid = t["id"]
        query = t["query"]
        gold = t["gold_passage"]
        print(f"\n=== q{qid:02d}: {query[:70]}...", flush=True)

        for branch in ("branch_a", "branch_b"):
            ids = t["stages"][branch]["selected_chunk_ids"]
            chunks = fetch_chunks(con, ids)
            ctx = build_context_string(chunks)
            user_msg = f"Context passages:\n{ctx}\n\nQuestion: {query}"
            print(f"  {branch}: {len(chunks)} chunks → Groq...", flush=True)
            try:
                llm_out = call_groq(api_key, args.model, system_prompt, user_msg)
            except Exception as e:
                print(f"  {branch} FAILED: {e}", flush=True)
                continue
            time.sleep(2.5)  # gentle throttle for free-tier RPM limits
            total_in  += llm_out["prompt_tokens"]
            total_out += llm_out["completion_tokens"]
            print(f"  {branch} done: {llm_out['elapsed_ms']}ms, {llm_out['completion_tokens']} tok", flush=True)
            t["stages"][branch]["prompt_full"] = f"[system]\n{system_prompt}\n\n[user]\n{user_msg}"
            t["stages"][branch]["prompt_chars"] = len(system_prompt) + len(user_msg)
            t["stages"][branch]["answer"] = llm_out["answer"]
            t["stages"][branch]["answer_chars"] = llm_out["answer_chars"]
            t["stages"][branch]["prompt_tokens"] = llm_out["prompt_tokens"]
            t["stages"][branch]["completion_tokens"] = llm_out["completion_tokens"]
            t["stages"][branch]["elapsed_ms"] = llm_out["elapsed_ms"]
            t["stages"][branch]["gold_match"] = gold_match(llm_out["answer"], gold)

        t["prompt_variant"] = args.prompt
        t["llm_provider"] = "groq"
        t["llm_model"] = args.model
        (out / tf.name).write_text(json.dumps(t, indent=2))

    print(f"\n[done] {len(trace_files)} traces → {out}")
    print(f"[tokens] in={total_in:,}  out={total_out:,}")


if __name__ == "__main__":
    main()
