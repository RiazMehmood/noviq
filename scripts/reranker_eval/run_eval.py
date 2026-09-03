"""Reranker A/B eval harness — pure Python.

Mirrors production retrieval pipeline (src-tauri/src/services/query_router.rs
+ llm.rs) using the same model artifacts. Test-only — no production code
touched.

Pipeline per question:
  1. Dense top-K via sqlite-vec on chunk_embeddings (vec_distance_cosine).
  2. FTS5 top-K via chunks_fts MATCH with the same tokenizer/stoplist as prod.
  3. Hybrid RRF fusion (k=60), dedup.
  4. Branch A: build_context (token-budgeted top-K) → LLM.
  5. Branch B: rerank pool with bge-reranker-base → build_context → LLM.
  6. Emit q<NN>.trace.json with full per-stage detail.

Constants mirror production defaults at the time of writing:
  CONTEXT_TOP_K=12, CONTEXT_TOKEN_BUDGET=2400, ECHO_VEC_K=24, ECHO_FTS_K=24,
  PER_CHUNK_CAP_FRAC=0.25, RRF k=60, reranker MAX_PAIR_TOKENS=256.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sqlite3
import struct
import sys
import time
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

# ─── Paths ────────────────────────────────────────────────────────────────────
ROOT = Path("/media/riaz/New Volume/echo-AI")
DB_PATH = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
EVAL_JSONL = ROOT / "data/reranker/v1/eval.jsonl"
EMBEDDER_NAME = "nomic-ai/nomic-embed-text-v1.5"
FASTEMBED_CACHE = ROOT / ".fastembed_cache"
RERANKER_DIR = ROOT / "models/bge-reranker-base"  # has model.onnx + tokenizer.json
LLM_GGUF = ROOT / "models/sindh_qwen2.5-0.5b-instruct.Q4_K_M.gguf"

# ─── Production constants (from src-tauri/src/services/query_router.rs + llm.rs)
CONTEXT_TOP_K = 12               # qr.rs:22
CONTEXT_TOKEN_BUDGET = 2400      # qr.rs:33
VEC_K = 24                       # CONTEXT_TOP_K * 2
FTS_K = 24
RRF_K = 60                       # qr.rs:2156
PER_CHUNK_CAP_FRAC = 0.25        # qr.rs:86
RERANKER_MAX_TOKENS = 256        # reranker.rs:10
MIN_CHUNK_LEN_FOR_FTS = 150      # qr.rs:1157 (LENGTH(c.content) > 150)

# Production FTS5 stoplist (qr.rs:1118-1125)
FTS_STOP = {
    "the","a","an","of","and","or","is","are","was","were","be","been","being",
    "to","in","on","at","by","for","with","as","from","that","this","these",
    "those","it","its","into","such","than","then","what","which","who","whom",
    "whose","how","when","where","why","do","does","did","can","could","shall",
    "should","will","would","may","might","must","under","over","about","any",
    "all","also","not","no","but","if","so","because","while","until",
}

# Production system prompt v5.2 (llm.rs:92)
GROUNDING_SYSTEM_PROMPT = (
    "Grounded Document Assistant v5.2\n\n"
    "You answer the user's question using ONLY the retrieved passages in the Context block.\n"
    "NEVER use outside KNOWLEDGE, PRIOR TRAINING, ASSUMPTIONS, or INTERPRETATIONS.\n"
    "Examine each passage carefully before deciding it is irrelevant, IF irrelevant reply with ONLY this single line and nothing else:\n"
    '"The retrieved documents do not contain enough information to answer this question.\n'
    "RULES\n"
    "- Cite every factual sentence with a bracketed source index, e.g. [1] or [2], matching the number shown in the Context block. Place the marker at the end of the sentence, before the period.\n"
    "- CRITICAL: Only use numbers that appear in the Context passages provided. If you see [1], [2], [3] in the context, you MUST NOT use [4] or higher. Incorrect numbering will result in your answer being rejected.\n"
    "- Use only information explicitly present in the passages. Do not invent, infer, or paraphrase beyond the text.\n"
    "- Preserve all conditions, deadlines, exceptions, and limitations exactly as written.\n"
    "- If passages conflict, present both versions and cite both indices; do not resolve.\n"
    "- Use plain language, short sentences, and numbered steps where appropriate. Neutral, factual tone.\n"
    "- Do not output a References section or list sources at the end — the UI renders them separately.\n"
    "- Think silently. Do not reveal reasoning or chain-of-thought. Output only the final answer."
)


# ─── Data classes ─────────────────────────────────────────────────────────────
@dataclass
class Chunk:
    chunk_id: str
    source_id: str
    file_name: str
    page: Optional[int]
    content: str           # full body
    unit_type: str         # leaf / section / document


@dataclass
class DenseHit:
    chunk: Chunk
    cosine: float          # higher = more similar (1 - distance)
    rank: int


@dataclass
class FtsHit:
    chunk: Chunk
    bm25: float            # lower = better (sqlite rank)
    rank: int
    matched_terms: list[str]


@dataclass
class HybridHit:
    chunk: Chunk
    fused_score: float
    fused_rank: int
    dense_rank: Optional[int]
    bm25_rank: Optional[int]


@dataclass
class RerankPair:
    chunk_id: str
    rerank_score: float
    rerank_rank: int
    hybrid_rank: int
    delta_rank: int
    pair_chars: int
    truncated_estimate: bool


# ─── DB / embedder / reranker / LLM init ──────────────────────────────────────
def open_db(db_path: Path) -> sqlite3.Connection:
    import sqlite_vec
    con = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    con.enable_load_extension(True)
    sqlite_vec.load(con)
    con.enable_load_extension(False)
    con.row_factory = sqlite3.Row
    return con


def load_embedder():
    from fastembed import TextEmbedding
    print(f"[init] loading embedder {EMBEDDER_NAME} (fastembed)", flush=True)
    return TextEmbedding(model_name=EMBEDDER_NAME, cache_dir=str(FASTEMBED_CACHE))


def load_reranker_cross_encoder():
    # bge-reranker-base via HF Hub (the cache copy); same weights as the ONNX
    # used in production. Float precision may differ by ~1e-6 — irrelevant.
    from sentence_transformers import CrossEncoder
    print(f"[init] loading reranker BAAI/bge-reranker-base", flush=True)
    return CrossEncoder("BAAI/bge-reranker-base", max_length=RERANKER_MAX_TOKENS)


def load_llm():
    from llama_cpp import Llama
    print(f"[init] loading LLM {LLM_GGUF.name}", flush=True)
    return Llama(
        model_path=str(LLM_GGUF),
        n_ctx=4096,
        n_threads=max(1, (os.cpu_count() or 4) // 2),
        seed=42,
        verbose=False,
        logits_all=False,
    )


# ─── Retrieval ────────────────────────────────────────────────────────────────
def embed_query(model, q: str) -> bytes:
    """Return float32 little-endian blob — sqlite-vec format.

    Uses fastembed's query_embed() so Nomic's required `search_query:` prefix
    is applied, matching production (fastembed-rs handles this internally on
    the document side at index time).
    """
    import numpy as np
    v = next(iter(model.query_embed([q]))).astype("float32")
    assert v.shape == (768,), f"unexpected dim {v.shape}"
    return v.tobytes()


def dense_top_k(con: sqlite3.Connection, qvec_blob: bytes, k: int) -> list[DenseHit]:
    rows = con.execute(
        """
        SELECT c.id, c.source_id, c.content, c.page_number, c.unit_type,
               COALESCE(s.file_name, s.file_path, 'unknown') AS file_name,
               vec_distance_cosine(ce.embedding, ?) AS dist
        FROM chunk_embeddings ce
        JOIN chunks c ON c.id = ce.chunk_id
        JOIN sources s ON s.id = c.source_id
        ORDER BY dist ASC
        LIMIT ?
        """,
        (qvec_blob, k),
    ).fetchall()
    return [
        DenseHit(
            chunk=Chunk(
                chunk_id=r["id"],
                source_id=r["source_id"],
                file_name=r["file_name"],
                page=r["page_number"],
                content=r["content"],
                unit_type=r["unit_type"],
            ),
            cosine=1.0 - float(r["dist"]),
            rank=i + 1,
        )
        for i, r in enumerate(rows)
    ]


def build_fts_query(q_text: str) -> tuple[str, list[str]]:
    """Mirror production FTS5 sanitizer (qr.rs:1126-1140).

    Returns (fts_query_string, kept_tokens_for_logging).
    """
    raw = re.split(r"[^A-Za-z0-9']+", q_text)
    tokens: list[str] = []
    for t in raw:
        t = t.strip().lower()
        if len(t) < 3:
            continue
        if t in FTS_STOP:
            continue
        tokens.append(t)
    if not tokens:
        # Degenerate fallback — quote the whole question.
        return f'"{q_text.replace(chr(34), "")}"', []
    return " OR ".join(f'"{t}"' for t in tokens), tokens


def fts5_top_k(con: sqlite3.Connection, q_text: str, k: int) -> tuple[list[FtsHit], str, list[str]]:
    fts_q, matched = build_fts_query(q_text)
    rows = con.execute(
        """
        SELECT c.id, c.source_id, c.content, c.page_number, c.unit_type,
               COALESCE(s.file_name, s.file_path, 'unknown') AS file_name,
               rank AS bm25
        FROM chunks_fts
        JOIN chunks c ON c.rowid = chunks_fts.rowid
        JOIN sources s ON s.id = c.source_id
        WHERE chunks_fts MATCH ? AND LENGTH(c.content) > ?
        ORDER BY rank LIMIT ?
        """,
        (fts_q, MIN_CHUNK_LEN_FOR_FTS, k),
    ).fetchall()
    hits = [
        FtsHit(
            chunk=Chunk(
                chunk_id=r["id"],
                source_id=r["source_id"],
                file_name=r["file_name"],
                page=r["page_number"],
                content=r["content"],
                unit_type=r["unit_type"],
            ),
            bm25=float(r["bm25"]),
            rank=i + 1,
            matched_terms=[t for t in matched if t in r["content"].lower()],
        )
        for i, r in enumerate(rows)
    ]
    return hits, fts_q, matched


def rrf_merge(dense: list[DenseHit], fts: list[FtsHit], k: int = RRF_K) -> list[HybridHit]:
    """Reciprocal Rank Fusion — mirrors qr.rs:rrf_merge."""
    score: dict[str, float] = {}
    chunks: dict[str, Chunk] = {}
    dense_rank_of: dict[str, int] = {}
    bm25_rank_of: dict[str, int] = {}

    for hit in fts:
        cid = hit.chunk.chunk_id
        score[cid] = score.get(cid, 0.0) + 1.0 / (k + hit.rank)
        chunks.setdefault(cid, hit.chunk)
        bm25_rank_of[cid] = hit.rank
    for hit in dense:
        cid = hit.chunk.chunk_id
        score[cid] = score.get(cid, 0.0) + 1.0 / (k + hit.rank)
        chunks.setdefault(cid, hit.chunk)
        dense_rank_of[cid] = hit.rank

    items = sorted(score.items(), key=lambda kv: kv[1], reverse=True)
    return [
        HybridHit(
            chunk=chunks[cid],
            fused_score=sc,
            fused_rank=i + 1,
            dense_rank=dense_rank_of.get(cid),
            bm25_rank=bm25_rank_of.get(cid),
        )
        for i, (cid, sc) in enumerate(items)
    ]


def build_context_topk(ranked: list[HybridHit], k: int, token_budget: int) -> list[HybridHit]:
    """Mirrors qr.rs:build_context — token-budget gating + per-chunk cap."""
    selected: list[HybridHit] = []
    seen: set[str] = set()
    total = 0
    per_chunk_cap = int(token_budget * PER_CHUNK_CAP_FRAC)
    for hit in ranked:
        if len(selected) >= k:
            break
        if hit.chunk.chunk_id in seen:
            continue
        est = len(hit.chunk.content) // 4 + 20
        if est > per_chunk_cap:
            continue
        if selected and total + est > token_budget:
            continue
        seen.add(hit.chunk.chunk_id)
        total += est
        selected.append(hit)
    return selected


# ─── Reranker ─────────────────────────────────────────────────────────────────
def rerank(ce, query: str, pool: list[HybridHit]) -> list[RerankPair]:
    pairs = [(query, h.chunk.content) for h in pool]
    scores = ce.predict(pairs, show_progress_bar=False)
    order = sorted(range(len(pool)), key=lambda i: -float(scores[i]))
    rank_of: dict[int, int] = {idx: r + 1 for r, idx in enumerate(order)}
    out: list[RerankPair] = []
    for i, h in enumerate(pool):
        # Truncation estimate: whitespace-tokens > MAX (BGE tokenizer ≈ 1 token/word
        # for English; rough upper-bound check).
        pair_words = len(query.split()) + len(h.chunk.content.split())
        out.append(RerankPair(
            chunk_id=h.chunk.chunk_id,
            rerank_score=float(scores[i]),
            rerank_rank=rank_of[i],
            hybrid_rank=h.fused_rank,
            delta_rank=h.fused_rank - rank_of[i],
            pair_chars=len(query) + len(h.chunk.content),
            truncated_estimate=pair_words > RERANKER_MAX_TOKENS,
        ))
    return out


# ─── Prompt + LLM ─────────────────────────────────────────────────────────────
def build_context_string(chunks: list[HybridHit]) -> str:
    parts = []
    for i, h in enumerate(chunks, start=1):
        page = f", page {h.chunk.page}" if h.chunk.page else ""
        parts.append(f"[{i}] {h.chunk.file_name}{page}\n{h.chunk.content}")
    return "\n\n---\n\n".join(parts)


def build_rag_prompt(query: str, context: str) -> str:
    """Mirrors qr.rs:build_rag_prompt — empty history, no focus_hint, English-only."""
    return (
        f"<|im_start|>system\n{GROUNDING_SYSTEM_PROMPT}<|im_end|>\n"
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
    elapsed_ms = int((time.time() - t0) * 1000)
    text = res["choices"][0]["text"]
    return {
        "answer": text,
        "answer_chars": len(text),
        "prompt_tokens": res["usage"]["prompt_tokens"],
        "completion_tokens": res["usage"]["completion_tokens"],
        "elapsed_ms": elapsed_ms,
    }


# ─── Scoring (gold-passage findability + answer match) ────────────────────────
def normalize_ws(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip().lower()


def gold_match(answer: str, gold: str) -> dict:
    """Cheap heuristics — substring overlap + bigram overlap proxy for ROUGE-L."""
    a = normalize_ws(answer)
    g = normalize_ws(gold)
    # Take a 40-char middle slice from gold as the substring probe.
    mid = max(0, len(g) // 2 - 20)
    probe = g[mid:mid + 40]
    substring_hit = probe in a if probe else False
    # Bigram Jaccard
    def bigrams(t: str) -> set:
        toks = t.split()
        return set(zip(toks, toks[1:]))
    ba, bg = bigrams(a), bigrams(g)
    bj = (len(ba & bg) / len(ba | bg)) if ba and bg else 0.0
    return {"substring_hit": substring_hit, "bigram_jaccard": round(bj, 3)}


# ─── Per-question runner ──────────────────────────────────────────────────────
def run_question(idx: int, query: str, gold: str, con, embedder, reranker_ce, llm, out_dir: Path):
    print(f"\n=== q{idx:02d}: {query[:80]}...", flush=True)
    t0 = time.time()

    # 1. Dense
    t = time.time()
    qvec_blob = embed_query(embedder, query)
    dense = dense_top_k(con, qvec_blob, VEC_K)
    dense_ms = int((time.time() - t) * 1000)
    print(f"  dense: {len(dense)} hits in {dense_ms}ms", flush=True)

    # 2. FTS5
    t = time.time()
    fts, fts_q, matched_terms = fts5_top_k(con, query, FTS_K)
    fts_ms = int((time.time() - t) * 1000)
    print(f"  fts5:  {len(fts)} hits in {fts_ms}ms", flush=True)

    # 3. Hybrid
    t = time.time()
    hybrid = rrf_merge(dense, fts)
    hybrid_ms = int((time.time() - t) * 1000)
    print(f"  hybrid: {len(hybrid)} unique chunks in {hybrid_ms}ms", flush=True)

    # 4. Branch A — top-K under budget, no rerank
    sel_a = build_context_topk(hybrid, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET)
    ctx_a = build_context_string(sel_a)
    prompt_a = build_rag_prompt(query, ctx_a)
    print(f"  branch A: {len(sel_a)} chunks; prompt {len(prompt_a)} chars; calling LLM...", flush=True)
    llm_a = run_llm(llm, prompt_a)
    print(f"  branch A done: {llm_a['elapsed_ms']}ms, {llm_a['completion_tokens']} tokens", flush=True)

    # 5. Branch B — rerank entire hybrid pool, then top-K
    t = time.time()
    rer = rerank(reranker_ce, query, hybrid)
    rerank_ms = int((time.time() - t) * 1000)
    # Reorder hybrid by rerank_rank
    rerank_pos: dict[str, int] = {p.chunk_id: p.rerank_rank for p in rer}
    reranked: list[HybridHit] = sorted(hybrid, key=lambda h: rerank_pos[h.chunk.chunk_id])
    sel_b = build_context_topk(reranked, CONTEXT_TOP_K, CONTEXT_TOKEN_BUDGET)
    ctx_b = build_context_string(sel_b)
    prompt_b = build_rag_prompt(query, ctx_b)
    print(f"  branch B: rerank {rerank_ms}ms; {len(sel_b)} chunks; calling LLM...", flush=True)
    llm_b = run_llm(llm, prompt_b)
    print(f"  branch B done: {llm_b['elapsed_ms']}ms, {llm_b['completion_tokens']} tokens", flush=True)

    # 6. Scoring
    match_a = gold_match(llm_a["answer"], gold)
    match_b = gold_match(llm_b["answer"], gold)

    # Find rank of gold passage in dense / hybrid (by content overlap, since gold
    # is a chunk-text snippet not a chunk_id).
    gold_snip = normalize_ws(gold)[len(gold)//2:len(gold)//2+80]
    def find_gold_rank(hits: list) -> Optional[int]:
        for h in hits:
            content_n = normalize_ws(h.chunk.content if hasattr(h, 'chunk') else h.content)
            if gold_snip in content_n:
                return h.rank if hasattr(h, 'rank') else h.fused_rank
        return None
    gold_rank_dense = find_gold_rank(dense)
    gold_rank_fts = find_gold_rank(fts)
    gold_rank_hybrid_a = find_gold_rank(hybrid)
    gold_rank_b_in_reranked = None
    for i, h in enumerate(reranked, start=1):
        if gold_snip in normalize_ws(h.chunk.content):
            gold_rank_b_in_reranked = i
            break

    total_ms = int((time.time() - t0) * 1000)
    trace = {
        "id": idx,
        "query": query,
        "gold_passage": gold,
        "gold_passage_chars": len(gold),
        "gold_ranks": {
            "dense": gold_rank_dense,
            "fts5": gold_rank_fts,
            "hybrid_pre_rerank": gold_rank_hybrid_a,
            "after_rerank": gold_rank_b_in_reranked,
        },
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "stages": {
            "dense": {
                "embedder": EMBEDDER_NAME,
                "elapsed_ms": dense_ms,
                "hits": [
                    {
                        "rank": h.rank, "chunk_id": h.chunk.chunk_id,
                        "cosine": round(h.cosine, 4),
                        "source": h.chunk.file_name, "page": h.chunk.page,
                        "unit_type": h.chunk.unit_type,
                        "content": h.chunk.content,
                    }
                    for h in dense
                ],
            },
            "fts5": {
                "fts_query": fts_q,
                "matched_terms": matched_terms,
                "elapsed_ms": fts_ms,
                "hits": [
                    {
                        "rank": h.rank, "chunk_id": h.chunk.chunk_id,
                        "bm25": round(h.bm25, 4),
                        "matched_terms_in_chunk": h.matched_terms,
                        "source": h.chunk.file_name, "page": h.chunk.page,
                        "unit_type": h.chunk.unit_type,
                        "content": h.chunk.content,
                    }
                    for h in fts
                ],
            },
            "hybrid": {
                "method": "RRF", "k": RRF_K,
                "elapsed_ms": hybrid_ms,
                "hits": [
                    {
                        "rank": h.fused_rank, "chunk_id": h.chunk.chunk_id,
                        "fused_score": round(h.fused_score, 6),
                        "dense_rank": h.dense_rank, "bm25_rank": h.bm25_rank,
                        "source": h.chunk.file_name, "page": h.chunk.page,
                        "unit_type": h.chunk.unit_type,
                        "content": h.chunk.content,
                    }
                    for h in hybrid
                ],
            },
            "branch_a": {
                "top_k_to_llm": len(sel_a),
                "selected_chunk_ids": [h.chunk.chunk_id for h in sel_a],
                "prompt_full": prompt_a,
                "prompt_chars": len(prompt_a),
                "answer": llm_a["answer"],
                "answer_chars": llm_a["answer_chars"],
                "prompt_tokens": llm_a["prompt_tokens"],
                "completion_tokens": llm_a["completion_tokens"],
                "elapsed_ms": llm_a["elapsed_ms"],
                "gold_match": match_a,
            },
            "branch_b": {
                "reranker_model": "BAAI/bge-reranker-base",
                "reranker_max_tokens": RERANKER_MAX_TOKENS,
                "rerank_elapsed_ms": rerank_ms,
                "pairs": [asdict(p) for p in rer],
                "top_k_to_llm": len(sel_b),
                "selected_chunk_ids": [h.chunk.chunk_id for h in sel_b],
                "prompt_full": prompt_b,
                "prompt_chars": len(prompt_b),
                "answer": llm_b["answer"],
                "answer_chars": llm_b["answer_chars"],
                "prompt_tokens": llm_b["prompt_tokens"],
                "completion_tokens": llm_b["completion_tokens"],
                "elapsed_ms": llm_b["elapsed_ms"],
                "gold_match": match_b,
            },
            "verdict": {
                "gold_in_a_topk": any(
                    gold_snip in normalize_ws(h.chunk.content) for h in sel_a
                ),
                "gold_in_b_topk": any(
                    gold_snip in normalize_ws(h.chunk.content) for h in sel_b
                ),
                "answer_winner": (
                    "b" if match_b["substring_hit"] and not match_a["substring_hit"]
                    else "a" if match_a["substring_hit"] and not match_b["substring_hit"]
                    else "tie" if match_a["substring_hit"] == match_b["substring_hit"]
                    else "tie"
                ),
                "rerank_latency_overhead_ms": rerank_ms + (llm_b["elapsed_ms"] - llm_a["elapsed_ms"]),
                "truncation_count": sum(1 for p in rer if p.truncated_estimate),
            },
        },
        "total_elapsed_ms": total_ms,
    }
    (out_dir / f"q{idx:02d}.trace.json").write_text(json.dumps(trace, indent=2))
    return trace


# ─── Main ─────────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--smoke", type=int, default=0, help="if N>0, only run first N questions")
    ap.add_argument("--out", type=str, default=None)
    args = ap.parse_args()

    ts = datetime.now().strftime("%Y%m%dT%H%M%S")
    out_dir = Path(args.out) if args.out else (ROOT / "logs/eval" / ts)
    out_dir.mkdir(parents=True, exist_ok=True)
    print(f"[out] {out_dir}", flush=True)

    # Load eval set
    eval_rows = [json.loads(line) for line in EVAL_JSONL.read_text().splitlines()]
    if args.smoke > 0:
        eval_rows = eval_rows[: args.smoke]
    print(f"[eval] {len(eval_rows)} questions", flush=True)

    # Init
    con = open_db(DB_PATH)
    chunk_count = con.execute("SELECT COUNT(*) FROM chunks").fetchone()[0]
    print(f"[db] {chunk_count} chunks", flush=True)
    embedder = load_embedder()
    reranker_ce = load_reranker_cross_encoder()
    llm = load_llm()

    manifest = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "db_path": str(DB_PATH),
        "db_chunks": chunk_count,
        "embedder": EMBEDDER_NAME,
        "reranker": "BAAI/bge-reranker-base",
        "llm_gguf": str(LLM_GGUF),
        "eval_set": str(EVAL_JSONL),
        "eval_set_sha256": hashlib.sha256(EVAL_JSONL.read_bytes()).hexdigest(),
        "n_questions": len(eval_rows),
        "constants": {
            "CONTEXT_TOP_K": CONTEXT_TOP_K,
            "CONTEXT_TOKEN_BUDGET": CONTEXT_TOKEN_BUDGET,
            "VEC_K": VEC_K, "FTS_K": FTS_K, "RRF_K": RRF_K,
            "PER_CHUNK_CAP_FRAC": PER_CHUNK_CAP_FRAC,
            "RERANKER_MAX_TOKENS": RERANKER_MAX_TOKENS,
        },
        "seed": 42,
        "llm_temperature": 0.0,
    }
    (out_dir / "run_manifest.json").write_text(json.dumps(manifest, indent=2))

    results_path = out_dir / "results.jsonl"
    with results_path.open("w") as rf:
        for i, row in enumerate(eval_rows):
            trace = run_question(
                i, row["query"], row["pos"][0],
                con, embedder, reranker_ce, llm, out_dir,
            )
            summary = {
                "id": i,
                "query": row["query"][:120],
                "gold_ranks": trace["gold_ranks"],
                "answer_winner": trace["stages"]["verdict"]["answer_winner"],
                "rerank_overhead_ms": trace["stages"]["verdict"]["rerank_latency_overhead_ms"],
                "truncation_count": trace["stages"]["verdict"]["truncation_count"],
                "match_a": trace["stages"]["branch_a"]["gold_match"],
                "match_b": trace["stages"]["branch_b"]["gold_match"],
                "total_ms": trace["total_elapsed_ms"],
            }
            rf.write(json.dumps(summary) + "\n")
            rf.flush()

    print(f"\n[done] {len(eval_rows)} traces written to {out_dir}", flush=True)


if __name__ == "__main__":
    main()
