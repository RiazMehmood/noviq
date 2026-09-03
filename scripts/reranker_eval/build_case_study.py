"""Build a comprehensive case-study walkthrough of the RAG pipeline.

For each test question, walks through every stage of the pipeline in order:
  1. The user's question
  2. The 'gold' (correct) passage we wanted to retrieve
  3. Step A — Dense (vector) retrieval: top-K with cosine scores
  4. Step B — FTS5 (keyword) retrieval: top-K with BM25 scores + matched terms
  5. Step C — Hybrid fusion (RRF): merged + re-ranked list
  6. Step D — Pipeline-A (no reranker): top-K selected for LLM with token budget
  7. Step E — The exact prompt sent to the LLM
  8. Step F — The LLM's answer
  9. Step G — Pipeline-B (with reranker): rerank scores, new ordering
 10. Step H — Top-K after rerank → LLM prompt → LLM answer

Output: a single markdown file suitable for reading or PDF export.
"""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
SOURCE = ROOT / "logs/eval/nomic_full"
OUT = Path("/home/riaz/Desktop/case_study_rag_pipeline.md")
RAGAS_CSV: Path | None = None  # if set, per-question RAGAS scores rendered inline

CHUNK_DISPLAY_CAP = 1200  # chars per chunk shown in the doc


def norm(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def short_content(c: str) -> str:
    c = norm(c)
    if len(c) <= CHUNK_DISPLAY_CAP:
        return c
    return c[:CHUNK_DISPLAY_CAP] + f"  …[truncated, full length {len(c)} chars]"


def fmt_chunk_block(idx: int, hit: dict, score_label: str, score: float, extras: str = "") -> list[str]:
    L = []
    page = f", page {hit['page']}" if hit.get("page") else ""
    L.append(f"**{idx}.** _{hit['source']}{page}_  — `{hit['chunk_id'][:8]}…`  ·  {score_label}: **{score:.4f}**{extras}")
    L.append("")
    L.append("> " + short_content(hit["content"]).replace("\n", "\n> "))
    L.append("")
    return L


def render_question(t: dict) -> str:
    L = []
    P = L.append
    qid = t["id"]
    q = t["query"]
    gold = t["gold_passage"]

    P(f"# Question {qid:02d}")
    P("")
    P(f"## Step 1 — User's question")
    P("")
    P(f"> **{q}**")
    P("")
    P(f"## Step 2 — The 'gold' (correct) passage we wanted to find")
    P("")
    P(f"This is the passage that, if retrieved, lets the LLM answer correctly. Hand-curated by a human; the pipeline doesn't know about it.")
    P("")
    P("```")
    P(norm(gold)[:2000] + ("..." if len(gold) > 2000 else ""))
    P("```")
    P("")

    # ── STEP 3 — DENSE ────────────────────────────────────────────────────────
    dense = t["stages"]["dense"]
    P(f"## Step 3 — Dense (vector / meaning) retrieval")
    P("")
    P(f"_Embedder: `{dense['embedder']}` · top-{len(dense['hits'])} returned in {dense['elapsed_ms']} ms._")
    P("")
    P("**How it works:** the question is turned into a 768-number vector by the Nomic model; ")
    P("every chunk in the database already has its own 768-number vector. We sort all chunks by ")
    P("how close their vector is to the question's vector (cosine similarity, higher = closer).")
    P("")
    dense_gold = t["gold_ranks"]["dense"]
    if dense_gold:
        P(f"✅ Gold passage was found at **rank {dense_gold}** in this list.")
    else:
        P(f"❌ Gold passage was **not found** in the dense top-{len(dense['hits'])}.")
    P("")
    P(f"### Dense top-{len(dense['hits'])} candidates")
    P("")
    for i, h in enumerate(dense["hits"], 1):
        L.extend(fmt_chunk_block(i, h, "cosine", h["cosine"]))

    # ── STEP 4 — FTS5 ─────────────────────────────────────────────────────────
    fts = t["stages"]["fts5"]
    P(f"## Step 4 — FTS5 (keyword / lexical) retrieval")
    P("")
    P(f"_SQLite FTS5 with porter stemmer · top-{len(fts['hits'])} returned in {fts['elapsed_ms']} ms._")
    P("")
    P("**How it works:** the question is split into words, stop-words removed (≥3 chars), and the ")
    P("remaining words are searched against the full-text index of every chunk. SQLite's BM25 ")
    P("ranks chunks by how often + how rarely the matched words appear (lower BM25 = better in SQLite).")
    P("")
    P(f"**FTS5 query string used:** `{fts['fts_query']}`")
    P("")
    P(f"**Words searched for:** `{', '.join(fts['matched_terms']) or '(none — fallback to quoted question)'}`")
    P("")
    fts_gold = t["gold_ranks"]["fts5"]
    if fts_gold:
        P(f"✅ Gold passage was found at **rank {fts_gold}** in this list.")
    else:
        P(f"❌ Gold passage was **not found** in the FTS5 top-{len(fts['hits'])}.")
    P("")
    P(f"### FTS5 top-{len(fts['hits'])} candidates")
    P("")
    for i, h in enumerate(fts["hits"], 1):
        matched = ", ".join(h.get("matched_terms_in_chunk", []))
        extras = f"  ·  matched words: _{matched}_" if matched else ""
        L.extend(fmt_chunk_block(i, h, "BM25", h["bm25"], extras))

    # ── STEP 5 — HYBRID FUSION ────────────────────────────────────────────────
    hyb = t["stages"]["hybrid"]
    P(f"## Step 5 — Hybrid fusion (combining meaning + keyword)")
    P("")
    P(f"_Method: Reciprocal Rank Fusion (RRF) with k=60 · {len(hyb['hits'])} unique chunks after merge._")
    P("")
    P("**How it works:** we don't trust either signal alone. Dense search understands meaning but ")
    P("misses exact terminology; keyword search nails exact terms but misses paraphrases. RRF combines ")
    P("them by giving each chunk a score = `1/(60 + dense_rank) + 1/(60 + bm25_rank)`. A chunk that ")
    P("appears high in *both* lists gets the highest combined score.")
    P("")
    hyb_gold = t["gold_ranks"]["hybrid_pre_rerank"]
    if hyb_gold:
        P(f"✅ Gold passage was found at **rank {hyb_gold}** after fusion.")
    else:
        P(f"❌ Gold passage was **not found** after fusion.")
    P("")
    P(f"### Hybrid top-{min(len(hyb['hits']), 20)} (showing first 20 of {len(hyb['hits'])})")
    P("")
    for i, h in enumerate(hyb["hits"][:20], 1):
        d_rank = h.get("dense_rank")
        b_rank = h.get("bm25_rank")
        sources = []
        if d_rank: sources.append(f"dense #{d_rank}")
        if b_rank: sources.append(f"FTS5 #{b_rank}")
        extras = f"  ·  came from: {' + '.join(sources) or '?'}"
        L.extend(fmt_chunk_block(i, h, "RRF score", h["fused_score"], extras))

    # ── STEP 6 — PIPELINE A: TOP-K NO RERANKER ────────────────────────────────
    ba = t["stages"]["branch_a"]
    P(f"## Step 6 — Pipeline A (no reranker): select top-K for the LLM")
    P("")
    P(f"_Budget gating: ≤12 chunks, ≤2400 prompt tokens, ≤25% of budget per chunk._")
    P("")
    P("**How it works:** walk down the hybrid list, adding chunks one at a time, stopping when ")
    P("we've added 12 or hit the token budget. Skip any chunk so long it would eat more than ")
    P("25% of the budget by itself.")
    P("")
    P(f"### {len(ba['selected_chunk_ids'])} chunks selected for Pipeline-A prompt:")
    P("")
    # Map chunk_id back to hybrid hit for content
    by_id = {h["chunk_id"]: h for h in hyb["hits"]}
    for i, cid in enumerate(ba["selected_chunk_ids"], 1):
        h = by_id.get(cid)
        if not h:
            P(f"**{i}.** _(chunk {cid[:8]}… not in cached hybrid pool)_")
            P("")
            continue
        L.extend(fmt_chunk_block(i, h, "RRF score", h["fused_score"]))

    # ── STEP 7 — EXACT PROMPT (A) ─────────────────────────────────────────────
    P(f"## Step 7 — Exact prompt sent to the LLM (Pipeline A)")
    P("")
    P(f"_Prompt size: {ba['prompt_chars']} characters, ~{ba['prompt_tokens']} tokens._")
    P("")
    P("```")
    P(ba["prompt_full"])
    P("```")
    P("")

    # ── STEP 8 — LLM ANSWER (A) ───────────────────────────────────────────────
    P(f"## Step 8 — LLM's answer (Pipeline A)")
    P("")
    P(f"_Generated in {ba['elapsed_ms']/1000:.1f}s · {ba['completion_tokens']} tokens · ")
    P(f"substring-hit vs gold: {'✅ yes' if ba['gold_match']['substring_hit'] else '❌ no'} · ")
    P(f"bigram-Jaccard vs gold: {ba['gold_match']['bigram_jaccard']:.3f}_")
    P("")
    P("```")
    P(ba["answer"].strip() or "(empty)")
    P("```")
    P("")

    # ── STEP 9 — PIPELINE B: RERANKER SCORES ──────────────────────────────────
    bb = t["stages"]["branch_b"]
    P(f"## Step 9 — Pipeline B: reranker scores every hybrid chunk")
    P("")
    P(f"_Reranker: `{bb['reranker_model']}` · cap: {bb['reranker_max_tokens']} tokens per (query, chunk) pair · scored in {bb['rerank_elapsed_ms']} ms._")
    P("")
    P("**How it works:** for each hybrid candidate, we send (question, chunk text) together through ")
    P("a small classification model that outputs one number — its 'relevance score' — between roughly ")
    P("-10 and +10. We re-sort the candidates by this score, highest first. If the pair is longer than ")
    P("256 tokens, the model only sees the first 256 (this is the truncation issue).")
    P("")
    n_trunc = sum(1 for p in bb["pairs"] if p.get("truncated_estimate"))
    P(f"**Truncation:** {n_trunc} of {len(bb['pairs'])} pairs ({100*n_trunc/max(len(bb['pairs']),1):.0f}%) were too long; reranker only saw the start.")
    P("")
    rerank_gold = t["gold_ranks"]["after_rerank"]
    if rerank_gold and hyb_gold:
        if rerank_gold < hyb_gold:
            P(f"✅ Reranker **promoted** gold from rank {hyb_gold} → **{rerank_gold}**.")
        elif rerank_gold > hyb_gold:
            P(f"⚠️  Reranker **demoted** gold from rank {hyb_gold} → **{rerank_gold}** (worse).")
        else:
            P(f"= Gold rank unchanged at {hyb_gold}.")
    elif rerank_gold:
        P(f"✅ Reranker put gold at rank {rerank_gold}.")
    else:
        P(f"❌ Gold not found in reranker top list.")
    P("")
    P("### Rerank score table (ordered by new rank, top 20)")
    P("")
    P("| new rank | old (hybrid) rank | Δ | rerank score | source | truncated? |")
    P("|---|---|---|---|---|---|")
    # pairs are stored in hybrid order; sort by rerank_rank
    pairs_sorted = sorted(bb["pairs"], key=lambda p: p["rerank_rank"])[:20]
    for p in pairs_sorted:
        h = by_id.get(p["chunk_id"], {})
        src = h.get("source", "?")
        delta = -(p["delta_rank"])  # delta_rank in trace = new - old; negate so + means improved
        delta_str = f"+{delta}" if delta > 0 else str(delta) if delta < 0 else "0"
        trunc = "yes" if p.get("truncated_estimate") else "no"
        P(f"| {p['rerank_rank']} | {p['hybrid_rank']} | {delta_str} | {p['rerank_score']:.4f} | {src[:50]} | {trunc} |")
    P("")

    # ── STEP 10 — PIPELINE B: TOP-K POST RERANK ──────────────────────────────
    P(f"## Step 10 — Pipeline B: top-K after rerank, fed to LLM")
    P("")
    P(f"### {len(bb['selected_chunk_ids'])} chunks selected for Pipeline-B prompt:")
    P("")
    rerank_score_by_id = {p["chunk_id"]: p["rerank_score"] for p in bb["pairs"]}
    for i, cid in enumerate(bb["selected_chunk_ids"], 1):
        h = by_id.get(cid)
        if not h:
            P(f"**{i}.** _(chunk {cid[:8]}… not in cached hybrid pool)_")
            P("")
            continue
        score = rerank_score_by_id.get(cid, 0.0)
        L.extend(fmt_chunk_block(i, h, "rerank score", score))

    # ── STEP 11 — EXACT PROMPT (B) ────────────────────────────────────────────
    P(f"## Step 11 — Exact prompt sent to the LLM (Pipeline B)")
    P("")
    P(f"_Prompt size: {bb['prompt_chars']} characters, ~{bb['prompt_tokens']} tokens._")
    P("")
    P("```")
    P(bb["prompt_full"])
    P("```")
    P("")

    # ── STEP 12 — LLM ANSWER (B) ──────────────────────────────────────────────
    P(f"## Step 12 — LLM's answer (Pipeline B)")
    P("")
    P(f"_Generated in {bb['elapsed_ms']/1000:.1f}s · {bb['completion_tokens']} tokens · ")
    P(f"substring-hit vs gold: {'✅ yes' if bb['gold_match']['substring_hit'] else '❌ no'} · ")
    P(f"bigram-Jaccard vs gold: {bb['gold_match']['bigram_jaccard']:.3f}_")
    P("")
    P("```")
    P(bb["answer"].strip() or "(empty)")
    P("```")
    P("")

    # ── PER-QUESTION VERDICT ─────────────────────────────────────────────────
    P(f"## Step 13 — Verdict for Question {qid:02d}")
    P("")
    a_hit = ba["gold_match"]["substring_hit"]
    b_hit = bb["gold_match"]["substring_hit"]
    winner = "Pipeline A (no reranker)" if a_hit and not b_hit else "Pipeline B (with reranker)" if b_hit and not a_hit else "tie"
    P(f"- Pipeline A answer matched gold substring: {'✅' if a_hit else '❌'}")
    P(f"- Pipeline B answer matched gold substring: {'✅' if b_hit else '❌'}")
    P(f"- Gold rank (dense): {t['gold_ranks']['dense'] or 'not found'}")
    P(f"- Gold rank (FTS5): {t['gold_ranks']['fts5'] or 'not found'}")
    P(f"- Gold rank (hybrid): {t['gold_ranks']['hybrid_pre_rerank'] or 'not found'}")
    P(f"- Gold rank (after rerank): {t['gold_ranks']['after_rerank'] or 'not found'}")
    P(f"- Winner: **{winner}**")
    P("")
    P("---")
    P("")
    return "\n".join(L)


def render_header() -> str:
    L = []
    P = L.append
    title_suffix = ""
    if SOURCE.name != "nomic_full":
        title_suffix = f" — {SOURCE.name}"
    P(f"# Case Study — EchoAI RAG Pipeline, Step-by-Step{title_suffix}")
    P("")
    P(f"_Generated 2026-05-18 from `logs/eval/{SOURCE.name}/` (23 questions, Pakistani legal / govt documents)._")
    P("")
    P("## What this document shows")
    P("")
    P("Each of 23 real test questions is walked through the **full retrieval-and-answer pipeline**, ")
    P("stage by stage. You will see exactly:")
    P("")
    P("1. What the user asked.")
    P("2. The correct ('gold') passage we hoped the system would find.")
    P("3. What the **meaning-based search** (dense vectors, Nomic embedder) found — with similarity scores.")
    P("4. What the **keyword search** (SQLite FTS5) found — with BM25 scores and the actual words matched.")
    P("5. How the two are **merged** (Reciprocal Rank Fusion).")
    P("6. Which chunks **made it into the LLM's prompt** under our token budget.")
    P("7. The **exact prompt** the LLM received — system instructions + passages + question.")
    P("8. The LLM's **answer** without any reranker reordering (Pipeline A).")
    P("9. How a **reranker** re-scores those same chunks — with the actual scores and which got promoted/demoted.")
    P("10. The LLM's answer **with reranker** reordering (Pipeline B).")
    P("11. A verdict per question.")
    P("")
    P("## The pipeline at a glance")
    P("")
    P("```")
    P("USER QUESTION")
    P("    │")
    P("    ├──► Dense (Nomic embedder)  ──► top-24 by cosine similarity")
    P("    │")
    P("    ├──► FTS5 (BM25 keyword)     ──► top-24 by BM25")
    P("    │")
    P("    └──► RRF Fusion              ──► combined ~40-48 unique chunks")
    P("                                       │")
    P("                  ┌────────────────────┴────────────────────┐")
    P("                  │                                         │")
    P("        Pipeline A: top-12                       Pipeline B: rerank all")
    P("        under 2400-tok budget                    with bge-reranker-base")
    P("                  │                                         │")
    P("                  ▼                                         ▼")
    P("            ChatML prompt                            ChatML prompt")
    P("                  │                                         │")
    P("                  ▼                                         ▼")
    is_groq = False
    try:
        first = json.loads(sorted(SOURCE.glob("q*.trace.json"))[0].read_text())
        is_groq = first.get("llm_provider") == "groq"
    except Exception:
        pass
    if is_groq:
        P("        Groq Llama-3.3-70B (hosted)                Groq Llama-3.3-70B (hosted)")
    else:
        P("        Qwen2.5-0.5B (fine-tuned)                   Qwen2.5-0.5B (fine-tuned)")
    P("                  │                                         │")
    P("              ANSWER A                                    ANSWER B")
    P("```")
    P("")
    P("## Models and parameters used")
    P("")
    P("| Component | Setting |")
    P("|---|---|")
    P("| Embedder | `nomic-ai/nomic-embed-text-v1.5` (768d) via fastembed |")
    P("| FTS5 | SQLite porter stemmer, 60-word stoplist, ≥3-char tokens, OR-fused |")
    P("| Fusion | Reciprocal Rank Fusion, k=60 |")
    P("| Context selection | top-12 chunks, ≤2400 prompt tokens, ≤25% budget per chunk |")
    P("| Reranker | `BAAI/bge-reranker-base` (zero-shot), 256-token pair cap |")
    # Detect LLM from first trace (groq runs set llm_provider/llm_model)
    llm_line = "Fine-tuned Qwen2.5-0.5B-Instruct, Q4_K_M GGUF, n_ctx=4096, temp=0"
    try:
        first = json.loads(sorted(SOURCE.glob("q*.trace.json"))[0].read_text())
        if first.get("llm_provider") == "groq":
            llm_line = f"Groq {first.get('llm_model', 'llama-3.x')} (hosted, temp=0) — q19-22 fallback to llama-3.1-8b-instant on daily quota"
    except Exception:
        pass
    P(f"| LLM | {llm_line} |")
    P("| Corpus | 1183 chunks across Pakistani Sindh govt PDFs (Constitution, Civil Service Rules, etc) |")
    P("")
    P("## Headline findings (full discussion at the end)")
    P("")
    P("- **Keyword search (FTS5) is the strongest single signal** on this corpus — recall@12 ≈ 0.70.")
    P("- **Meaning search (dense Nomic) works but is weaker** — recall@12 ≈ 0.35. Many documents use ")
    P("  domain-specific phrasing the general-purpose embedder doesn't strongly recognize.")
    P("- **Reranker (zero-shot) is a wash** — helps top-1 / top-5 (+0.04 / +0.08), hurts top-12 (−0.04). ")
    P("  Per-question swings are large in both directions.")
    P("- **The reranker truncation problem (86% of pairs over 256 tokens)** turned out NOT to be the root ")
    P("  cause — eliminating it via sliding windows made things slightly worse.")
    P("- **The 0.5B LLM paraphrases instead of quoting** verbatim regardless of prompt — answer-tokens-")
    P("  in-gold capped around 0.35 across all prompt variants.")
    P("")
    P("**Conclusion:** the reranker needs domain-specific fine-tuning (dataset ready at ")
    P("`data/reranker/v1/`). The LLM may also need to be a bigger model (1.5B or 3B) for verbatim quotes.")
    P("")
    P("---")
    P("")
    return "\n".join(L)


def render_footer(traces: list[dict]) -> str:
    L = []
    P = L.append
    P("# Cross-question summary")
    P("")
    P("## Where the gold passage landed at each stage (lower = better)")
    P("")
    P("| q | Dense | FTS5 | Hybrid (A) | After rerank (B) |")
    P("|---|---|---|---|---|")
    for t in traces:
        g = t["gold_ranks"]
        P(f"| q{t['id']:02d} | {g['dense'] or '—'} | {g['fts5'] or '—'} | {g['hybrid_pre_rerank'] or '—'} | {g['after_rerank'] or '—'} |")
    P("")

    # Aggregate metrics
    def at(ranks, k): return sum(1 for r in ranks if r is not None and r <= k) / len(ranks)
    def mrr(ranks): return sum(1/r for r in ranks if r is not None) / len(ranks)
    d  = [t["gold_ranks"]["dense"] for t in traces]
    f  = [t["gold_ranks"]["fts5"] for t in traces]
    ha = [t["gold_ranks"]["hybrid_pre_rerank"] for t in traces]
    hb = [t["gold_ranks"]["after_rerank"] for t in traces]
    P("## Retrieval quality (recall@K = % of questions where gold appears in top-K)")
    P("")
    P("| metric | Dense | FTS5 | Hybrid (A) | Hybrid + Rerank (B) |")
    P("|---|---|---|---|---|")
    P(f"| recall@1  | {at(d,1):.2f} | {at(f,1):.2f} | {at(ha,1):.2f} | {at(hb,1):.2f} |")
    P(f"| recall@5  | {at(d,5):.2f} | {at(f,5):.2f} | {at(ha,5):.2f} | {at(hb,5):.2f} |")
    P(f"| recall@12 | {at(d,12):.2f} | {at(f,12):.2f} | {at(ha,12):.2f} | {at(hb,12):.2f} |")
    P(f"| MRR       | {mrr(d):.2f} | {mrr(f):.2f} | {mrr(ha):.2f} | {mrr(hb):.2f} |")
    P("")
    P("## Discussion")
    P("")
    P("**Why FTS5 dominates on this corpus.** The Pakistani legal / govt documents use very specific ")
    P("vocabulary — 'civil servant', 'EOL', 'BPS-19', 'gazetted', 'SOR', 'notification' — that the ")
    P("user questions also use. Lexical overlap is high, so FTS5's BM25 ranking is hard to beat.")
    P("")
    P("**Why the dense embedder underperforms.** `nomic-embed-text-v1.5` is trained on general English ")
    P("web text. It maps the question and the chunk into a 768-dimensional space where 'meaning ")
    P("similarity' is measured. On legal / bureaucratic text, this space is too generic: many chunks ")
    P("about completely different topics look numerically similar because they share government-style ")
    P("phrasing. A domain-adapted embedder (e.g. fine-tuned on Sindh govt docs) would likely close ")
    P("most of the gap.")
    P("")
    P("**Why the reranker doesn't reliably help.** A reranker re-scores (question, chunk) pairs jointly, ")
    P("which *should* spot subtle relevance that single-encoder retrieval misses. But `bge-reranker-base` ")
    P("is zero-shot on this domain: it has never seen Pakistani service rules or constitutional articles ")
    P("paired with bureaucratic questions. Its reordering is approximately random with respect to ")
    P("ground truth — sometimes it gets lucky (q20: +25 ranks), sometimes it makes things worse ")
    P("(q04: −10 ranks). Fine-tuning on in-domain (query, positive, negative) triples is the standard ")
    P("fix; our dataset is already prepared.")
    P("")
    P("**Why the LLM's answers are short and ungrounded.** The fine-tuned Qwen2.5-0.5B is a very ")
    P("small model. It can follow simple instructions but cannot reliably perform the harder behaviour ")
    P("we want — find the relevant sentences in 12 long passages, quote them verbatim, and cite them by ")
    P("bracketed number. Prompt engineering (we tried strict 'quote-first' and a few-shot example) ")
    P("nudged answer length and structure but couldn't overcome the model's capacity ceiling.")
    P("")
    P("**Recommended next steps.**")
    P("")
    P("1. **Fine-tune `bge-reranker-base`** on the prepared `data/reranker/v1/` triples (198 train, ")
    P("   23 eval). Re-run this case study with the fine-tuned reranker; expect recall@5 / @12 to ")
    P("   improve materially.")
    P("2. **Consider a larger generator LLM** (Qwen2.5-1.5B or 3B) for the final answer step. The 0.5B ")
    P("   model is the right size for chat fluency but borderline for verbatim grounded answering.")
    P("3. **Adopt the v3 few-shot prompt** as the new baseline — it produced slightly more on-topic ")
    P("   answers than the v5.2 baseline.")
    P("4. **Monitor the dense embedder gap** — if recall@5 stays below FTS5 even with a fine-tuned ")
    P("   reranker, consider fine-tuning a domain-adapted embedder as well.")
    P("")
    return "\n".join(L)


def load_ragas_scores(csv_path: Path) -> dict[tuple[int, str], dict]:
    import csv as _csv
    out: dict[tuple[int, str], dict] = {}
    with csv_path.open() as f:
        for row in _csv.DictReader(f):
            try:
                qid = int(row["qid"])
            except (KeyError, ValueError):
                continue
            branch = row.get("branch", "")
            scores = {}
            for k in ("faithfulness", "answer_relevancy", "context_precision", "context_recall"):
                v = row.get(k)
                if v not in (None, "", "nan", "NaN"):
                    try:
                        scores[k] = float(v)
                    except ValueError:
                        pass
            out[(qid, branch)] = scores
    return out


def render_ragas_block(qid: int, ragas: dict) -> str:
    a = ragas.get((qid, "branch_a"), {})
    b = ragas.get((qid, "branch_b"), {})
    if not a and not b:
        return ""
    L = ["", "## RAGAS judge scores (Groq-judged)", ""]
    L.append("| metric | Branch A (no rerank) | Branch B (with rerank) |")
    L.append("|---|---|---|")
    for k in ("faithfulness", "answer_relevancy", "context_precision", "context_recall"):
        va = f"{a[k]:.3f}" if k in a else "—"
        vb = f"{b[k]:.3f}" if k in b else "—"
        L.append(f"| {k} | {va} | {vb} |")
    L.append("")
    L.append("_Higher = better (0..1). Judged by Groq `llama-3.1-8b-instant`._")
    L.append("")
    return "\n".join(L)


def main():
    global SOURCE, OUT, RAGAS_CSV
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", default=str(SOURCE), help="run dir under logs/eval/")
    ap.add_argument("--out", default=str(OUT), help="output markdown path")
    ap.add_argument("--ragas-csv", default=None, help="optional ragas_scores.csv to overlay per-question scores")
    args = ap.parse_args()
    SOURCE = Path(args.source)
    OUT = Path(args.out)
    if args.ragas_csv:
        RAGAS_CSV = Path(args.ragas_csv)

    ragas = load_ragas_scores(RAGAS_CSV) if RAGAS_CSV and RAGAS_CSV.exists() else {}

    trace_files = sorted(SOURCE.glob("q*.trace.json"))
    if not trace_files:
        raise SystemExit(f"no traces in {SOURCE}")
    traces = [json.loads(p.read_text()) for p in trace_files]

    parts = [render_header()]
    for t in traces:
        body = render_question(t)
        if ragas:
            body += "\n" + render_ragas_block(t["id"], ragas)
        parts.append(body)
    parts.append(render_footer(traces))

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(parts))
    size_kb = OUT.stat().st_size / 1024
    print(f"[done] wrote {OUT} ({size_kb:.0f} KB, {len(traces)} questions, ragas={'yes' if ragas else 'no'})")


if __name__ == "__main__":
    main()
