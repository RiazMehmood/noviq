# scripts/reranker_dataset/stage_d_mine.py
"""Stage D: hard-negative mining via dense + BM25 fusion."""
import json
import random
import numpy as np
from pathlib import Path
from tqdm import tqdm

from scripts.reranker_dataset.config import (
    OUT_DIR, MINING_TOP_K, SCORE_FLOOR, SCORE_CEILING,
    NEGATIVES_PER_POSITIVE, MIN_NEGATIVES_PER_POSITIVE, SEED,
)
from scripts.reranker_dataset.utils import db
from scripts.reranker_dataset.utils.embedder import Embedder
from scripts.reranker_dataset.utils.bm25 import BM25Index

def filter_score_band(candidates: list[tuple[str, float]], floor: float, ceiling: float) -> list[tuple[str, float]]:
    return [(cid, s) for cid, s in candidates if floor <= s <= ceiling]

def sample_negatives(candidates: list[tuple[str, float]], n: int, seed: int) -> list[tuple[str, float]]:
    rng = random.Random(seed)
    if len(candidates) <= n:
        return list(candidates)
    return rng.sample(candidates, n)

def load_positives() -> list[dict]:
    p = OUT_DIR / "positives.jsonl"
    rows: list[dict] = []
    with p.open("r", encoding="utf-8") as f:
        for line in f:
            rows.append(json.loads(line))
    return rows

def main() -> int:
    positives = load_positives()
    chunks = db.load_chunks()
    chunk_ids = [c["id"] for c in chunks]
    chunk_texts = [c["content"] for c in chunks]
    chunk_by_id = {c["id"]: c for c in chunks}
    id_to_idx = {cid: i for i, cid in enumerate(chunk_ids)}

    print(f"Encoding {len(chunk_texts)} chunks with BGE...")
    emb = Embedder()
    chunk_vecs = emb.encode(chunk_texts)  # (N, 768), normalized

    print("Building BM25 index...")
    bm25 = BM25Index(chunk_texts)

    out_path = OUT_DIR / "triples_raw.jsonl"
    rng = random.Random(SEED)
    kept = 0
    dropped_low_neg = 0

    with out_path.open("w", encoding="utf-8") as f:
        for pos in tqdm(positives, desc="mining"):
            q = pos["query"]
            pos_id = pos["positive_chunk_id"]
            q_vec = emb.encode([q])[0]
            dense_scores = chunk_vecs @ q_vec  # cosine since normalized
            dense_top = np.argsort(-dense_scores)[: MINING_TOP_K * 2]

            bm25_top = bm25.top_k(q, k=MINING_TOP_K * 2)
            bm25_top_ids = {chunk_ids[i] for i, _ in bm25_top}

            # Fuse: union of top dense + top BM25, exclude positive
            cand_ids = {chunk_ids[i] for i in dense_top} | bm25_top_ids
            cand_ids.discard(pos_id)

            # Score each candidate by its dense sim to the query (single normalized scale)
            cand_scored = [(cid, float(dense_scores[id_to_idx[cid]])) for cid in cand_ids]
            banded = filter_score_band(cand_scored, SCORE_FLOOR, SCORE_CEILING)
            if len(banded) < MIN_NEGATIVES_PER_POSITIVE:
                dropped_low_neg += 1
                continue
            negs = sample_negatives(banded, NEGATIVES_PER_POSITIVE, seed=rng.randint(0, 1 << 30))

            f.write(json.dumps({
                "query": q,
                "positive_chunk_id": pos_id,
                "positive_text": pos["positive_text"],
                "negatives": [
                    {"chunk_id": cid, "text": chunk_by_id[cid]["content"], "score": s}
                    for cid, s in negs
                ],
                "origin": pos["origin"],
            }) + "\n")
            kept += 1

    print(f"\nMined {kept} triples ({dropped_low_neg} dropped for too few hard negatives)")
    print(f"Wrote {out_path}")
    return 0

if __name__ == "__main__":
    import sys
    sys.exit(main())
