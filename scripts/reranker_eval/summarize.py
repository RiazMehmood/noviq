"""Roll up all q<NN>.trace.json files in a run dir into eval_summary.md.

Two sections:
  1. Plain-English: "We tested N questions. Reranker helped on X, hurt on Y..."
  2. Numeric tables: recall@K, MRR, answer match, reranker diagnostics, per-q.
"""
from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path


def load_traces(run_dir: Path) -> list[dict]:
    return [json.loads(p.read_text()) for p in sorted(run_dir.glob("q*.trace.json"))]


def gold_rank_after(rank: int | None, k: int) -> bool:
    return rank is not None and rank <= k


def mrr(ranks: list[int | None]) -> float:
    vals = [1.0 / r for r in ranks if r is not None]
    return sum(vals) / len(ranks) if ranks else 0.0


def recall_at_k(ranks: list[int | None], k: int) -> float:
    hits = sum(1 for r in ranks if r is not None and r <= k)
    return hits / len(ranks) if ranks else 0.0


def render_summary(traces: list[dict], manifest: dict) -> str:
    n = len(traces)
    # Collect ranks
    dense_ranks = [t["gold_ranks"]["dense"] for t in traces]
    fts_ranks   = [t["gold_ranks"]["fts5"] for t in traces]
    hyb_ranks_a = [t["gold_ranks"]["hybrid_pre_rerank"] for t in traces]
    hyb_ranks_b = [t["gold_ranks"]["after_rerank"] for t in traces]

    match_a = [t["stages"]["branch_a"]["gold_match"]["substring_hit"] for t in traces]
    match_b = [t["stages"]["branch_b"]["gold_match"]["substring_hit"] for t in traces]
    bj_a    = [t["stages"]["branch_a"]["gold_match"]["bigram_jaccard"] for t in traces]
    bj_b    = [t["stages"]["branch_b"]["gold_match"]["bigram_jaccard"] for t in traces]

    rerank_overhead = [t["stages"]["verdict"]["rerank_latency_overhead_ms"] for t in traces]
    rerank_ms       = [t["stages"]["branch_b"]["rerank_elapsed_ms"] for t in traces]
    trunc_counts    = [t["stages"]["verdict"]["truncation_count"] for t in traces]
    trunc_totals    = [len(t["stages"]["branch_b"]["pairs"]) for t in traces]

    # Winner buckets
    winners = {"a": 0, "b": 0, "tie": 0}
    for ma, mb in zip(match_a, match_b):
        if ma and not mb: winners["a"] += 1
        elif mb and not ma: winners["b"] += 1
        else: winners["tie"] += 1

    # Rerank effect on gold
    deltas = []
    for t in traces:
        a, b = t["gold_ranks"]["hybrid_pre_rerank"], t["gold_ranks"]["after_rerank"]
        if a is not None and b is not None:
            deltas.append(b - a)
    promoted = sum(1 for d in deltas if d < 0)
    demoted  = sum(1 for d in deltas if d > 0)
    same     = sum(1 for d in deltas if d == 0)

    L = []
    P = L.append

    P(f"# Reranker A/B Eval — {manifest.get('timestamp_utc','?')}")
    P("")
    P(f"- Eval set: `{manifest.get('eval_set','?')}` "
      f"(sha256 {manifest.get('eval_set_sha256','?')[:12]}...)")
    P(f"- Corpus: {manifest.get('db_chunks','?')} chunks")
    P(f"- Embedder: `{manifest.get('embedder','?')}`")
    P(f"- Reranker: `{manifest.get('reranker','?')}` (zero-shot)")
    P(f"- LLM: `{Path(manifest.get('llm_gguf','?')).name}` @ temperature=0")
    P("")
    P("## In plain English")
    P("")
    P(f"We tested the app on {n} questions about Pakistani legal/government documents.")
    P(f"For each question we let the app answer twice — once **without** the reranker,")
    P(f"once **with** it — using the same retrieved passages.")
    P("")
    P(f"- The reranker **helped** on **{winners['b']}** questions.")
    P(f"- It **hurt** on **{winners['a']}** questions.")
    P(f"- It made **no difference** on **{winners['tie']}** questions "
      f"(both matched, or neither matched).")
    if rerank_ms:
        P(f"- It added on average **{statistics.mean(rerank_ms)/1000:.1f} seconds** per question.")
    P("")
    trunc_rate = (sum(trunc_counts) / max(sum(trunc_totals), 1)) * 100
    P(f"**Key issue spotted:** on **{trunc_rate:.0f}%** of (question, passage) pairs the")
    P(f"reranker could only read the start of the passage because it was too long.")
    P(f"Fixing this (shorter chunks, or fine-tuning the reranker on this domain)")
    P(f"is likely to improve results.")
    P("")
    # Recommendation logic
    r5_a = recall_at_k(hyb_ranks_a, 5)
    r5_b = recall_at_k(hyb_ranks_b, 5)
    delta_r5 = r5_b - r5_a
    if trunc_rate > 25:
        rec = ("**Recommendation:** the reranker is heavily truncated on this "
               "corpus — re-chunk or fine-tune on shorter passages BEFORE drawing "
               "conclusions from a zero-shot reranker eval.")
    elif delta_r5 >= 0.05:
        rec = ("**Recommendation:** proceed with reranker fine-tune — even "
               "zero-shot the reranker shows positive retrieval signal.")
    elif delta_r5 <= 0:
        rec = ("**Recommendation:** zero-shot reranker is not helping on this "
               "corpus — fine-tune is required before relying on it in production.")
    else:
        rec = "**Recommendation:** marginal gain; fine-tune to confirm value."
    P(rec)
    P("")

    P("## The numbers")
    P("")
    P("### Retrieval quality (gold passage found in top-K)")
    P("")
    P("| metric              | Dense | FTS5 | Hybrid (A) | Hybrid+Rerank (B) | Δ (B−A) |")
    P("|---|---|---|---|---|---|")
    for kk in (1, 5, 12):
        rd = recall_at_k(dense_ranks, kk)
        rf = recall_at_k(fts_ranks, kk)
        ra = recall_at_k(hyb_ranks_a, kk)
        rb = recall_at_k(hyb_ranks_b, kk)
        P(f"| recall@{kk:<2}          | {rd:.2f}  | {rf:.2f} | {ra:.2f}      | {rb:.2f}              | {rb-ra:+.2f}  |")
    P(f"| MRR                 | {mrr(dense_ranks):.2f}  | {mrr(fts_ranks):.2f} | "
      f"{mrr(hyb_ranks_a):.2f}      | {mrr(hyb_ranks_b):.2f}              | {mrr(hyb_ranks_b)-mrr(hyb_ranks_a):+.2f}  |")
    P("")
    if deltas:
        P(f"Reranker effect on gold passage rank:  "
          f"**{promoted} promoted**, **{demoted} demoted**, **{same} unchanged**.")
        P(f"Mean Δrank: **{statistics.mean(deltas):+.2f}** (negative = better).")
    P("")

    P("### Answer quality (vs gold passage text)")
    P("")
    P("| metric                    | Branch A | Branch B | Δ |")
    P("|---|---|---|---|")
    a_hit = sum(match_a) / n
    b_hit = sum(match_b) / n
    P(f"| substring hit             | {a_hit:.2f}    | {b_hit:.2f}    | {b_hit-a_hit:+.2f} |")
    P(f"| bigram-Jaccard (mean)     | {statistics.mean(bj_a):.3f}   | {statistics.mean(bj_b):.3f}   | {statistics.mean(bj_b)-statistics.mean(bj_a):+.3f} |")
    P("")

    P("### Reranker diagnostics")
    P("")
    P(f"- Pairs scored total:        **{sum(trunc_totals)}**  ({n} q × ~{sum(trunc_totals)//max(n,1)} candidates)")
    P(f"- Truncated (>~256 words):   **{sum(trunc_counts)}** ({trunc_rate:.1f}%)")
    P(f"- Mean rerank latency:       **{statistics.mean(rerank_ms):.0f} ms/question**")
    P(f"- Mean total overhead:       **{statistics.mean(rerank_overhead):.0f} ms/question** "
      f"(rerank + Branch B LLM delta)")
    P(f"- Gold demoted by rerank:    **{demoted}/{len(deltas)}** questions where gold was rerankable")
    P("")

    P("### Per-question table")
    P("")
    P("| q | gold rank dense | gold rank FTS5 | gold rank hybrid | gold rank after rerank | A→B winner | trunc |")
    P("|---|---|---|---|---|---|---|")
    for t in traces:
        gr = t["gold_ranks"]
        ma = t["stages"]["branch_a"]["gold_match"]["substring_hit"]
        mb = t["stages"]["branch_b"]["gold_match"]["substring_hit"]
        winner = "A" if ma and not mb else "B" if mb and not ma else "tie"
        tc = t["stages"]["verdict"]["truncation_count"]
        tt = len(t["stages"]["branch_b"]["pairs"])
        P(f"| q{t['id']:02d} | {gr['dense'] or '—'} | {gr['fts5'] or '—'} | {gr['hybrid_pre_rerank'] or '—'} | {gr['after_rerank'] or '—'} | {winner} | {tc}/{tt} |")
    P("")
    return "\n".join(L)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("run_dir")
    args = ap.parse_args()
    rd = Path(args.run_dir)
    if not rd.is_dir():
        sys.exit(f"not a dir: {rd}")
    traces = load_traces(rd)
    if not traces:
        sys.exit(f"no q*.trace.json in {rd}")
    manifest_p = rd / "run_manifest.json"
    manifest = json.loads(manifest_p.read_text()) if manifest_p.exists() else {}
    out = rd / "eval_summary.md"
    out.write_text(render_summary(traces, manifest))
    print(f"[summary] {out}")


if __name__ == "__main__":
    main()
