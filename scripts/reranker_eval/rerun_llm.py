"""Cheap LLM-only re-run with a swapped system prompt.

Reuses retrieval+rerank results from an existing trace dir. For each question,
we re-fetch the selected chunks from echo.db by id, format them with a new
prompt variant, call the LLM, and write a fresh trace tree.

Total cost ≈ 2 × N_questions LLM calls. No embedding, no reranker, no DB scans
beyond a single chunks-by-id lookup per question.

Usage:
  python rerun_llm.py --source logs/eval/nomic_full --variant v2_quote_first \
                      --out logs/eval/prompt_v2
"""
from __future__ import annotations

import argparse
import json
import sqlite3
import sys
import time
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
DB_PATH = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
LLM_GGUF = ROOT / "models/sindh_qwen2.5-0.5b-instruct.Q4_K_M.gguf"

# ─── Prompt variants ──────────────────────────────────────────────────────────
PROMPTS = {
    "v5_2_baseline": (
        "Grounded Document Assistant v5.2\n\n"
        "You answer the user's question using ONLY the retrieved passages in the Context block.\n"
        "NEVER use outside KNOWLEDGE, PRIOR TRAINING, ASSUMPTIONS, or INTERPRETATIONS.\n"
        "Examine each passage carefully before deciding it is irrelevant, IF irrelevant reply with ONLY this single line and nothing else:\n"
        '"The retrieved documents do not contain enough information to answer this question.\n'
        "RULES\n"
        "- Cite every factual sentence with a bracketed source index, e.g. [1] or [2].\n"
        "- Use only information explicitly present in the passages.\n"
        "- Preserve all conditions, deadlines, exceptions, and limitations exactly as written.\n"
        "- Think silently. Do not reveal reasoning. Output only the final answer."
    ),

    "v2_quote_first": (
        "You answer questions about Pakistani legal/government documents using ONLY the supplied passages.\n\n"
        "HOW TO ANSWER:\n"
        "1. Find the sentence(s) in the passages that directly answer the question.\n"
        "2. Quote them VERBATIM (copy the exact words, do not paraphrase).\n"
        "3. After each quoted sentence add a citation marker [N] matching the passage number.\n"
        "4. If no passage answers the question, reply with ONLY:\n"
        '   "The retrieved documents do not contain enough information to answer this question."\n\n'
        "DO NOT add explanations, summaries, or background. Just the quoted answer + citation."
    ),

    "v3_few_shot": (
        "You answer questions using ONLY the supplied Context passages. Quote the relevant text "
        "and cite the passage number.\n\n"
        "EXAMPLE\n"
        "Context passages:\n"
        "[1] Civil Servants Act, page 4\n"
        "Section 9. Probation — A person on initial appointment shall be on probation for a "
        "period of two years which may be extended for one additional year.\n\n"
        "[2] Service Rules, page 12\n"
        "Confirmation in service is granted after successful completion of the probation period.\n\n"
        "Question: How long is the probation period for new civil servants?\n\n"
        "Answer: The probation period is two years, which may be extended for one additional year [1].\n\n"
        "END EXAMPLE\n\n"
        "RULES\n"
        "- Quote the exact wording from the passages. Do not paraphrase or invent.\n"
        "- Every factual sentence must end with [N] where N is the passage number.\n"
        "- Use only the numbers [1]..[K] that actually appear in the Context block.\n"
        "- If no passage answers the question, output ONLY:\n"
        '  "The retrieved documents do not contain enough information to answer this question."'
    ),
}


def load_llm():
    from llama_cpp import Llama
    print(f"[init] loading LLM {LLM_GGUF.name}", flush=True)
    return Llama(
        model_path=str(LLM_GGUF),
        n_ctx=4096,
        n_threads=8,
        n_gpu_layers=0,
        seed=42,
        verbose=False,
        logits_all=False,
    )


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


def build_context_string(chunks: list[dict]) -> str:
    parts = []
    for i, c in enumerate(chunks, start=1):
        page = f", page {c['page_number']}" if c.get("page_number") else ""
        parts.append(f"[{i}] {c['file_name']}{page}\n{c['content']}")
    return "\n\n---\n\n".join(parts)


def build_rag_prompt(system: str, query: str, context: str) -> str:
    return (
        f"<|im_start|>system\n{system}<|im_end|>\n"
        f"<|im_start|>user\n"
        f"Context passages:\n{context}\n\n"
        f"Question: {query}<|im_end|>\n"
        f"<|im_start|>assistant\n"
    )


def run_llm(llm, prompt: str, max_tokens: int = 1024) -> dict:
    t0 = time.time()
    res = llm(
        prompt,
        max_tokens=max_tokens,
        temperature=0.0,
        top_p=1.0,
        top_k=1,
        repeat_penalty=1.0,
        stop=["<|im_end|>", "<|im_start|>"],
    )
    return {
        "answer": res["choices"][0]["text"],
        "answer_chars": len(res["choices"][0]["text"]),
        "prompt_tokens": res["usage"]["prompt_tokens"],
        "completion_tokens": res["usage"]["completion_tokens"],
        "elapsed_ms": int((time.time() - t0) * 1000),
    }


def gold_match(answer: str, gold: str) -> dict:
    import re
    def norm(s): return re.sub(r"\s+", " ", s).strip().lower()
    a, g = norm(answer), norm(gold)
    mid = max(0, len(g) // 2 - 20)
    probe = g[mid:mid + 40]
    substring_hit = probe in a if probe else False
    def bigrams(t):
        toks = t.split()
        return set(zip(toks, toks[1:]))
    ba, bg = bigrams(a), bigrams(g)
    bj = (len(ba & bg) / len(ba | bg)) if ba and bg else 0.0
    return {"substring_hit": substring_hit, "bigram_jaccard": round(bj, 3)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True, help="Existing trace dir to reuse retrieval from")
    ap.add_argument("--variant", required=True, choices=list(PROMPTS.keys()))
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    src = Path(args.source)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    system_prompt = PROMPTS[args.variant]

    con = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row

    llm = load_llm()

    trace_files = sorted(src.glob("q*.trace.json"))
    if not trace_files:
        sys.exit(f"no traces in {src}")

    manifest = {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime()),
        "source_run": str(src),
        "prompt_variant": args.variant,
        "llm_gguf": str(LLM_GGUF),
        "note": "LLM-only re-run; retrieval/rerank reused from source_run",
    }
    (out / "run_manifest.json").write_text(json.dumps(manifest, indent=2))

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
            prompt = build_rag_prompt(system_prompt, query, ctx)
            print(f"  {branch}: {len(chunks)} chunks, calling LLM...", flush=True)
            llm_out = run_llm(llm, prompt)
            print(f"  {branch} done: {llm_out['elapsed_ms']}ms, {llm_out['completion_tokens']} tok", flush=True)
            t["stages"][branch]["prompt_full"] = prompt
            t["stages"][branch]["prompt_chars"] = len(prompt)
            t["stages"][branch]["answer"] = llm_out["answer"]
            t["stages"][branch]["answer_chars"] = llm_out["answer_chars"]
            t["stages"][branch]["prompt_tokens"] = llm_out["prompt_tokens"]
            t["stages"][branch]["completion_tokens"] = llm_out["completion_tokens"]
            t["stages"][branch]["elapsed_ms"] = llm_out["elapsed_ms"]
            t["stages"][branch]["gold_match"] = gold_match(llm_out["answer"], gold)

        t["prompt_variant"] = args.variant
        (out / tf.name).write_text(json.dumps(t, indent=2))

    print(f"\n[done] {len(trace_files)} traces → {out}")


if __name__ == "__main__":
    main()
