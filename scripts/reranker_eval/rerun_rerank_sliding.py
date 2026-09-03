"""Knob 1: sliding-window reranker.

Each hybrid candidate chunk is split into overlapping ~150-word windows.
Reranker scores every (query, window) pair; chunk gets MAX window score.
This bypasses the 256-token truncation that capped 86% of pairs in baseline.

Reuses cached hybrid pool from an existing run dir. Runs reranker + LLM only.

Usage:
  python rerun_rerank_sliding.py --source logs/eval/nomic_full \
      --prompt v3_few_shot --window 150 --stride 100 \
      --out logs/eval/knob1_sliding150
"""
from __future__ import annotations

import argparse
import json
import sqlite3
import sys
import time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
DB_PATH = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
RERANKER_HF = "BAAI/bge-reranker-base"
LLM_GGUF = ROOT / "models/sindh_qwen2.5-0.5b-instruct.Q4_K_M.gguf"

CONTEXT_TOP_K = 12
CONTEXT_TOKEN_BUDGET = 2400
PER_CHUNK_CAP_FRAC = 0.25

# Reuse v3 few-shot from Knob 3 (best of three prompts).
sys.path.insert(0, str(ROOT / "scripts/reranker_eval"))
from rerun_llm import PROMPTS, build_context_string, build_rag_prompt, run_llm, gold_match


@dataclass
class HitCache:
    chunk_id: str
    file_name: str
    page: object
    content: str
    fused_score: float
    fused_rank: int


def load_reranker():
    from sentence_transformers import CrossEncoder
    print(f"[init] loading reranker {RERANKER_HF}", flush=True)
    return CrossEncoder(RERANKER_HF, max_length=512)


def load_llm():
    from llama_cpp import Llama
    print(f"[init] loading LLM {LLM_GGUF.name}", flush=True)
    return Llama(
        model_path=str(LLM_GGUF),
        n_ctx=4096, n_threads=8, n_gpu_layers=0,
        seed=42, verbose=False, logits_all=False,
    )


def split_windows(text: str, window: int, stride: int) -> list[str]:
    """Word-level sliding windows. Returns at least one window."""
    words = text.split()
    if len(words) <= window:
        return [text]
    out = []
    i = 0
    while i < len(words):
        piece = " ".join(words[i:i + window])
        out.append(piece)
        if i + window >= len(words):
            break
        i += stride
    return out


def rerank_sliding(ce, query: str, hits: list[HitCache], window: int, stride: int):
    """Score each chunk by MAX over its windows. Returns list of (hit, max_score, n_windows)."""
    flat_pairs = []
    flat_owner = []
    win_counts = []
    for idx, h in enumerate(hits):
        wins = split_windows(h.content, window, stride)
        win_counts.append(len(wins))
        for w in wins:
            flat_pairs.append((query, w))
            flat_owner.append(idx)
    scores = ce.predict(flat_pairs, show_progress_bar=False)
    max_per_chunk = [-1e9] * len(hits)
    for owner, sc in zip(flat_owner, scores):
        s = float(sc)
        if s > max_per_chunk[owner]:
            max_per_chunk[owner] = s
    paired = list(zip(hits, max_per_chunk, win_counts))
    paired.sort(key=lambda t: -t[1])
    return paired, sum(win_counts)


def build_context_topk(hits: list[HitCache], k: int, token_budget: int) -> list[HitCache]:
    selected = []
    total = 0
    per_chunk_cap = int(token_budget * PER_CHUNK_CAP_FRAC)
    for h in hits:
        if len(selected) >= k:
            break
        est = len(h.content) // 4 + 20
        if est > per_chunk_cap:
            continue
        if selected and total + est > token_budget:
            continue
        total += est
        selected.append(h)
    return selected


def find_gold_rank(hits: list[HitCache], gold: str) -> int | None:
    import re
    g = re.sub(r"\s+", " ", gold).strip().lower()
    probe = g[max(0, len(g)//2 - 40): len(g)//2 + 40]
    for i, h in enumerate(hits):
        c = re.sub(r"\s+", " ", h.content).strip().lower()
        if probe in c:
            return i + 1
    return None


def hit_to_dict_for_ctx(h: HitCache) -> dict:
    return {"content": h.content, "file_name": h.file_name, "page_number": h.page}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True)
    ap.add_argument("--prompt", default="v3_few_shot", choices=list(PROMPTS.keys()))
    ap.add_argument("--window", type=int, default=150)
    ap.add_argument("--stride", type=int, default=100)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    src = Path(args.source)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    ce = load_reranker()
    llm = load_llm()
    system_prompt = PROMPTS[args.prompt]

    trace_files = sorted(src.glob("q*.trace.json"))
    if not trace_files:
        sys.exit(f"no traces in {src}")

    manifest = {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime()),
        "source_run": str(src),
        "knob": "knob1_sliding_rerank",
        "window_words": args.window,
        "stride_words": args.stride,
        "prompt_variant": args.prompt,
        "reranker": RERANKER_HF,
        "llm_gguf": str(LLM_GGUF),
    }
    (out / "run_manifest.json").write_text(json.dumps(manifest, indent=2))

    for tf in trace_files:
        t = json.loads(tf.read_text())
        qid = t["id"]
        query = t["query"]
        gold = t["gold_passage"]
        print(f"\n=== q{qid:02d}: {query[:70]}", flush=True)

        # Reuse cached hybrid pool
        hits = [
            HitCache(
                chunk_id=h["chunk_id"],
                file_name=h["source"],
                page=h.get("page"),
                content=h["content"],
                fused_score=h["fused_score"],
                fused_rank=h["rank"],
            )
            for h in t["stages"]["hybrid"]["hits"]
        ]

        # Branch A unchanged — keep existing answer from source trace (was v5.2);
        # for fair comparison re-run Branch A with the new prompt too.
        sel_a = build_context_topk(hits, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET)
        ctx_a = build_context_string([hit_to_dict_for_ctx(h) for h in sel_a])
        prompt_a = build_rag_prompt(system_prompt, query, ctx_a)
        print(f"  branch A: {len(sel_a)} chunks", flush=True)
        llm_a = run_llm(llm, prompt_a)
        print(f"  branch A done: {llm_a['elapsed_ms']}ms, {llm_a['completion_tokens']} tok", flush=True)

        # Branch B — sliding-window rerank
        t0 = time.time()
        reranked, total_windows = rerank_sliding(ce, query, hits, args.window, args.stride)
        rerank_ms = int((time.time() - t0) * 1000)
        reranked_hits = [r[0] for r in reranked]
        sel_b = build_context_topk(reranked_hits, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET)
        ctx_b = build_context_string([hit_to_dict_for_ctx(h) for h in sel_b])
        prompt_b = build_rag_prompt(system_prompt, query, ctx_b)
        print(f"  branch B: rerank {rerank_ms}ms, {total_windows} windows, {len(sel_b)} chunks", flush=True)
        llm_b = run_llm(llm, prompt_b)
        print(f"  branch B done: {llm_b['elapsed_ms']}ms, {llm_b['completion_tokens']} tok", flush=True)

        # Recompute gold ranks against the new ordering
        gold_rank_hybrid = find_gold_rank(hits, gold)
        gold_rank_rerank = find_gold_rank(reranked_hits, gold)

        # Update trace
        t["prompt_variant"] = args.prompt
        t["knob"] = "knob1_sliding_rerank"
        t["sliding"] = {"window": args.window, "stride": args.stride, "total_windows": total_windows}
        t["gold_ranks"]["hybrid_pre_rerank"] = gold_rank_hybrid
        t["gold_ranks"]["after_rerank"] = gold_rank_rerank

        t["stages"]["branch_a"]["selected_chunk_ids"] = [h.chunk_id for h in sel_a]
        t["stages"]["branch_a"]["prompt_full"] = prompt_a
        t["stages"]["branch_a"]["prompt_chars"] = len(prompt_a)
        t["stages"]["branch_a"]["answer"] = llm_a["answer"]
        t["stages"]["branch_a"]["answer_chars"] = llm_a["answer_chars"]
        t["stages"]["branch_a"]["prompt_tokens"] = llm_a["prompt_tokens"]
        t["stages"]["branch_a"]["completion_tokens"] = llm_a["completion_tokens"]
        t["stages"]["branch_a"]["elapsed_ms"] = llm_a["elapsed_ms"]
        t["stages"]["branch_a"]["gold_match"] = gold_match(llm_a["answer"], gold)

        t["stages"]["branch_b"]["rerank_elapsed_ms"] = rerank_ms
        t["stages"]["branch_b"]["selected_chunk_ids"] = [h.chunk_id for h in sel_b]
        t["stages"]["branch_b"]["prompt_full"] = prompt_b
        t["stages"]["branch_b"]["prompt_chars"] = len(prompt_b)
        t["stages"]["branch_b"]["answer"] = llm_b["answer"]
        t["stages"]["branch_b"]["answer_chars"] = llm_b["answer_chars"]
        t["stages"]["branch_b"]["prompt_tokens"] = llm_b["prompt_tokens"]
        t["stages"]["branch_b"]["completion_tokens"] = llm_b["completion_tokens"]
        t["stages"]["branch_b"]["elapsed_ms"] = llm_b["elapsed_ms"]
        t["stages"]["branch_b"]["gold_match"] = gold_match(llm_b["answer"], gold)
        # Rebuild simplified pairs with max-window score
        t["stages"]["branch_b"]["pairs"] = [
            {
                "chunk_id": h.chunk_id,
                "rerank_score": float(s),
                "rerank_rank": r + 1,
                "hybrid_rank": h.fused_rank,
                "delta_rank": (r + 1) - h.fused_rank,
                "windows": n_win,
                "truncated_estimate": False,  # windows fit by construction
            }
            for r, (h, s, n_win) in enumerate(reranked)
        ]
        t["stages"]["verdict"]["truncation_count"] = 0  # by construction

        (out / tf.name).write_text(json.dumps(t, indent=2))

    print(f"\n[done] {len(trace_files)} traces → {out}")


if __name__ == "__main__":
    main()
