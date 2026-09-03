"""Render q<NN>.trace.json into a dual-layer human-readable .log.

Layer 1: plain-English narrative (no jargon, no raw scores, no internal IDs,
no model names). A non-technical reviewer should be able to read only Layer 1
and summarize the result in two sentences.

Layer 2: full technical detail — all chunk text, every score, both prompts,
both answers.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import textwrap
from pathlib import Path


# ─── Plain-English bucket converters ──────────────────────────────────────────
def cosine_bucket(c: float) -> str:
    if c >= 0.85: return "very similar"
    if c >= 0.70: return "fairly similar"
    if c >= 0.55: return "somewhat similar"
    return "weakly similar"


def rerank_bucket(s: float) -> str:
    # bge-reranker raw logits range roughly -10..+10 in practice;
    # post-sigmoid scores cluster near 0 (irrelevant) or above 0.5 (relevant).
    # The harness records raw logits.
    if s >= 1.0: return "high"
    if s >= 0.0: return "medium"
    if s >= -3.0: return "low"
    return "very low"


def short_file(name: str) -> str:
    """Trim absolute paths to last 2 segments for readability."""
    parts = name.replace("\\", "/").split("/")
    if len(parts) >= 2:
        return "/".join(parts[-2:])
    return parts[-1]


def slug(s: str, max_words: int = 5) -> str:
    words = re.findall(r"[A-Za-z0-9]+", s.lower())[:max_words]
    return "_".join(words) or "untitled"


def wrap(text: str, width: int = 78, indent: str = "    ") -> str:
    """Wrap a body of text with an indent."""
    out = []
    for para in text.split("\n"):
        if not para.strip():
            out.append("")
            continue
        for line in textwrap.wrap(para, width=width):
            out.append(indent + line)
    return "\n".join(out)


def yn(b: bool) -> str:
    return "Yes" if b else "No"


# ─── Render a single trace ────────────────────────────────────────────────────
def render_trace(d: dict) -> str:
    qid = d["id"]
    q = d["query"]
    gold = d["gold_passage"]
    ranks = d["gold_ranks"]
    stages = d["stages"]
    dense = stages["dense"]
    fts = stages["fts5"]
    hyb = stages["hybrid"]
    a = stages["branch_a"]
    b = stages["branch_b"]
    v = stages["verdict"]

    lines: list[str] = []
    P = lines.append  # shorthand

    bar_eq = "=" * 70
    bar_dash = "-" * 70

    # ─── Header ───────────────────────────────────────────────────────────
    P(bar_eq)
    P(f"  QUESTION {qid:02d}                                {d['timestamp_utc'][:10]}")
    P(bar_eq)
    P("")
    P("THE QUESTION")
    P(wrap(q.strip()))
    P("")
    P("WHAT WE EXPECTED TO FIND (gold passage from our test set)")
    P(wrap(f'"{gold.strip()[:400]}{"..." if len(gold) > 400 else ""}"'))
    P(wrap(f"[passage is {len(gold)} characters]", indent="    "))
    P("")

    # ─── Layer 1 narrative ────────────────────────────────────────────────
    P(bar_dash)
    P("  HOW THE APP SEARCHED")
    P(bar_dash)
    P("")
    P("Step 1 — Meaning search (vector search)")
    P(wrap(
        "The app converted the question into a numeric 'fingerprint' and "
        "compared it against fingerprints of all passages in your documents. "
        f"The top {len(dense['hits'])} most similar passages were kept."
    ))
    P("")
    P("  Top 3 passages by meaning similarity:")
    for h in dense["hits"][:3]:
        page = f", page {h['page']}" if h['page'] else ""
        P(f"    {h['rank']}. {short_file(h['source'])}{page} "
          f"({cosine_bucket(h['cosine'])}, score {h['cosine']:.2f})")
        preview = re.sub(r"\s+", " ", h["content"]).strip()[:120]
        P(f"       \"{preview}...\"")
    if ranks["dense"]:
        P(f"  Expected passage appeared at position {ranks['dense']}.")
    else:
        P(f"  Expected passage did NOT appear in meaning search (top {len(dense['hits'])}).")
    P("")

    P("Step 2 — Keyword search (FTS5)")
    P(wrap(
        "Separately, the app searched for the actual words from the question "
        "inside the documents. This catches matches that meaning-search "
        "might miss."
    ))
    P(f"  Search words used: {', '.join(fts['matched_terms']) or '(none — all stopwords)'}")
    P("")
    P("  Top 3 passages by keyword match:")
    for h in fts["hits"][:3]:
        page = f", page {h['page']}" if h['page'] else ""
        P(f"    {h['rank']}. {short_file(h['source'])}{page}")
        preview = re.sub(r"\s+", " ", h["content"]).strip()[:120]
        P(f"       \"{preview}...\"")
    if ranks["fts5"]:
        P(f"  Expected passage appeared at position {ranks['fts5']}.")
    else:
        P(f"  Expected passage did NOT appear in keyword search (top {len(fts['hits'])}).")
    P("")

    P("Step 3 — Combining both searches")
    P(wrap(
        "The app merged both lists using Reciprocal Rank Fusion (RRF), which "
        "rewards passages that score well on either approach. The combined "
        f"top {len(hyb['hits'])} passages went to the next stage."
    ))
    if ranks["hybrid_pre_rerank"]:
        P(f"  Expected passage appeared at combined position "
          f"{ranks['hybrid_pre_rerank']}.")
    else:
        P("  Expected passage did NOT appear in combined search.")
    P("")

    P(bar_dash)
    P("  TWO WAYS TO PICK THE FINAL ANSWERS")
    P(bar_dash)
    P("")
    P(wrap(
        "We tested two approaches in parallel on the SAME combined list:"
    ))
    P("")
    P("  APPROACH A — Trust the combined search ranking.")
    P("              Take the top passages (under a length budget) as-is.")
    P("")
    P("  APPROACH B — Re-judge with a smarter model (the reranker).")
    P("              The reranker reads the question and each passage")
    P("              together and gives a fresh relevance score. The")
    P("              passages are then reordered.")
    P("")

    # Branch A
    P(bar_dash)
    P("  APPROACH A — Without reranker")
    P(bar_dash)
    P("")
    P(f"Passages sent to the AI: {len(a['selected_chunk_ids'])}")
    hyb_by_id = {h["chunk_id"]: h for h in hyb["hits"]}
    for i, cid in enumerate(a["selected_chunk_ids"], start=1):
        h = hyb_by_id[cid]
        page = f", page {h['page']}" if h['page'] else ""
        P(f"  {i}. {short_file(h['source'])}{page} "
          f"(originally at combined position {h['rank']})")
    P("")
    P("The AI's answer:")
    answer_a = a["answer"].strip() or "(empty)"
    for ln in answer_a.split("\n"):
        P(wrap(ln) if ln.strip() else "")
    P("")
    P("  Did this match what we expected?  "
      f"{'YES' if a['gold_match']['substring_hit'] else 'NO'}")
    P(f"  Bigram overlap with gold: {a['gold_match']['bigram_jaccard']}")
    P(f"  Time taken (LLM only): {a['elapsed_ms']/1000:.1f} seconds")
    P("")

    # Branch B
    P(bar_dash)
    P("  APPROACH B — With reranker")
    P(bar_dash)
    P("")
    # Top moves
    pairs_by_id = {p["chunk_id"]: p for p in b["pairs"]}
    moves = sorted(b["pairs"], key=lambda p: -abs(p["delta_rank"]))[:5]
    P("Biggest moves caused by the reranker:")
    for p in moves:
        if p["delta_rank"] == 0:
            continue
        h = hyb_by_id[p["chunk_id"]]
        page = f", page {h['page']}" if h['page'] else ""
        direction = "UP" if p["delta_rank"] > 0 else "DOWN"
        P(f"  • {short_file(h['source'])}{page}: "
          f"moved {direction} (combined position {p['hybrid_rank']} → "
          f"new position {p['rerank_rank']})")
    P("")
    P(f"Passages sent to the AI (new order): {len(b['selected_chunk_ids'])}")
    for i, cid in enumerate(b["selected_chunk_ids"], start=1):
        h = hyb_by_id[cid]; p = pairs_by_id[cid]
        page = f", page {h['page']}" if h['page'] else ""
        trunc = " — partial read by reranker" if p["truncated_estimate"] else ""
        P(f"  {i}. {short_file(h['source'])}{page} "
          f"(reranker rated {rerank_bucket(p['rerank_score'])}{trunc})")
    P("")
    P("The AI's answer:")
    answer_b = b["answer"].strip() or "(empty)"
    for ln in answer_b.split("\n"):
        P(wrap(ln) if ln.strip() else "")
    P("")
    P("  Did this match what we expected?  "
      f"{'YES' if b['gold_match']['substring_hit'] else 'NO'}")
    P(f"  Bigram overlap with gold: {b['gold_match']['bigram_jaccard']}")
    P(f"  Time taken (LLM only): {b['elapsed_ms']/1000:.1f} seconds")
    P(f"  Reranker added: {b['rerank_elapsed_ms']/1000:.1f} seconds")
    P("")

    # Verdict
    P(bar_dash)
    P("  VERDICT")
    P(bar_dash)
    P("")
    if a["gold_match"]["substring_hit"] and not b["gold_match"]["substring_hit"]:
        winner_text = "Approach A (without reranker) gave the better answer."
    elif b["gold_match"]["substring_hit"] and not a["gold_match"]["substring_hit"]:
        winner_text = "Approach B (with reranker) gave the better answer."
    elif a["gold_match"]["substring_hit"] and b["gold_match"]["substring_hit"]:
        winner_text = "Both approaches gave acceptable answers."
    else:
        winner_text = ("Neither approach produced an answer that matched the "
                       "expected text. Likely cause: the expected passage "
                       "wasn't in the chunks shown to the AI.")
    P(wrap(winner_text))
    P("")
    if ranks["after_rerank"] and ranks["hybrid_pre_rerank"]:
        delta = ranks["after_rerank"] - ranks["hybrid_pre_rerank"]
        if delta > 0:
            P(f"  Reranker DEMOTED the expected passage: combined position "
              f"{ranks['hybrid_pre_rerank']} → after-rerank position "
              f"{ranks['after_rerank']} (worse by {delta}).")
        elif delta < 0:
            P(f"  Reranker PROMOTED the expected passage: combined position "
              f"{ranks['hybrid_pre_rerank']} → after-rerank position "
              f"{ranks['after_rerank']} (better by {-delta}).")
        else:
            P(f"  Reranker did not change the expected passage's position "
              f"(stayed at {ranks['hybrid_pre_rerank']}).")
    trunc_n = sum(1 for p in b["pairs"] if p["truncated_estimate"])
    if trunc_n > 0:
        P(f"  Caveat: {trunc_n} of {len(b['pairs'])} passages were too long "
          f"for the reranker to read in full (more than ~256 words). The "
          f"reranker saw only the start of those passages.")
    P("")

    # ─── Layer 2: technical detail ────────────────────────────────────────
    P(bar_eq)
    P("  TECHNICAL DETAIL BELOW — skip if not debugging")
    P(bar_eq)
    P("")
    P(f"Run timestamp:           {d['timestamp_utc']}")
    P(f"Total time (whole pipe): {d['total_elapsed_ms']/1000:.1f} s")
    P("")

    # All hybrid chunks with full text
    P("─── All combined candidates (hybrid top-{}) ───".format(len(hyb["hits"])))
    for h in hyb["hits"]:
        page = f", page {h['page']}" if h['page'] else ""
        P("")
        P(f"  [hybrid_rank={h['rank']}] chunk_id={h['chunk_id']}")
        P(f"  source: {h['source']}{page}  unit_type={h['unit_type']}  "
          f"chars={len(h['content'])}")
        P(f"  scores: dense_rank={h['dense_rank']} bm25_rank={h['bm25_rank']} "
          f"fused_score={h['fused_score']:.6f}")
        if h["chunk_id"] in pairs_by_id:
            pp = pairs_by_id[h["chunk_id"]]
            P(f"  rerank: rank={pp['rerank_rank']} score={pp['rerank_score']:+.4f} "
              f"delta={pp['delta_rank']:+d} truncated={pp['truncated_estimate']}")
        P("  ─ content ─")
        P(wrap(h["content"], width=76))
    P("")

    # Branch A prompt + answer
    P("─── BRANCH A: FULL PROMPT SENT TO LLM ───")
    P("")
    for ln in a["prompt_full"].split("\n"):
        P("  " + ln)
    P("")
    P("─── BRANCH A: FULL LLM ANSWER ───")
    P(wrap(a["answer"] or "(empty)", width=76))
    P(f"  [prompt_tokens={a['prompt_tokens']} completion_tokens={a['completion_tokens']} "
      f"elapsed_ms={a['elapsed_ms']}]")
    P("")

    # Branch B prompt + answer
    P("─── BRANCH B: FULL PROMPT SENT TO LLM ───")
    P("")
    for ln in b["prompt_full"].split("\n"):
        P("  " + ln)
    P("")
    P("─── BRANCH B: FULL LLM ANSWER ───")
    P(wrap(b["answer"] or "(empty)", width=76))
    P(f"  [prompt_tokens={b['prompt_tokens']} completion_tokens={b['completion_tokens']} "
      f"elapsed_ms={b['elapsed_ms']} rerank_ms={b['rerank_elapsed_ms']}]")
    P("")

    # Full rerank table
    P("─── FULL RERANK TABLE (sorted by rerank_rank) ───")
    P(f"{'rerank':>6} {'hybrid':>6} {'score':>9} {'delta':>5} {'trunc':>5}  chunk_id")
    for p in sorted(b["pairs"], key=lambda x: x["rerank_rank"]):
        P(f"{p['rerank_rank']:>6} {p['hybrid_rank']:>6} {p['rerank_score']:>+9.4f} "
          f"{p['delta_rank']:>+5d} {str(p['truncated_estimate'])[0]:>5}  {p['chunk_id']}")
    P("")
    P(bar_eq)
    P(f"  END q{qid:02d}")
    P(bar_eq)
    return "\n".join(lines)


# ─── Main ─────────────────────────────────────────────────────────────────────
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("trace_path_or_dir",
                    help="path to one .trace.json or to a run dir containing them")
    ap.add_argument("--out-dir", default=None,
                    help="output dir (default = same dir as trace)")
    args = ap.parse_args()

    p = Path(args.trace_path_or_dir)
    if p.is_dir():
        traces = sorted(p.glob("q*.trace.json"))
    elif p.is_file():
        traces = [p]
    else:
        sys.exit(f"not found: {p}")

    out_dir = Path(args.out_dir) if args.out_dir else (p if p.is_dir() else p.parent)
    out_dir.mkdir(parents=True, exist_ok=True)

    for tp in traces:
        d = json.loads(tp.read_text())
        slug_s = slug(d["query"])
        out_path = out_dir / f"q{d['id']:02d}_{slug_s}.log"
        out_path.write_text(render_trace(d))
        print(f"[rendered] {out_path}")


if __name__ == "__main__":
    main()
