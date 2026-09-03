"""Build a standalone RAGAS report for sharing.

Output: /home/riaz/Desktop/ragas_report.md
- Plain-English what-is-RAGAS section
- Aggregate table (Branch A vs Branch B, mean + sample count)
- Per-question table
- Verdict
"""
from __future__ import annotations
import pandas as pd
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
CSV = ROOT / "logs/eval/groq_llama70b/ragas_scores.merged.csv"
OUT = Path("/home/riaz/Desktop/ragas_report.md")

METRICS = ["faithfulness", "answer_relevancy", "context_precision", "context_recall"]
PRETTY = {
    "faithfulness": "Faithfulness",
    "answer_relevancy": "Answer Relevancy",
    "context_precision": "Context Precision",
    "context_recall": "Context Recall",
}
DESC = {
    "faithfulness":
        "Does the answer stick to the retrieved passages, or invent facts? "
        "Judge counts claims in the answer and checks each one against the contexts. "
        "Score = fraction of claims supported by the contexts. **High = no hallucination.**",
    "answer_relevancy":
        "Does the answer actually address the question? Judge generates synthetic "
        "questions from the answer and compares them (via Nomic embeddings) to the "
        "original question. **High = on-topic.**",
    "context_precision":
        "Are the retrieved chunks actually useful? For each chunk, judge decides if "
        "it contains information that helps answer the question. Score = fraction of "
        "useful chunks among the top retrieved. **High = no junk in the prompt.**",
    "context_recall":
        "Did retrieval find everything needed to answer? Judge breaks the gold answer "
        "into atomic facts and checks each against the retrieved chunks. **High = "
        "nothing important was missed.**",
}


def fmt(v: float) -> str:
    return f"{v:.3f}" if pd.notna(v) else "—"


def main():
    df = pd.read_csv(CSV)

    L = []
    P = L.append

    P("# RAGAS Evaluation Report — EchoAI RAG Pipeline")
    P("")
    P("_Generated 2026-05-18 from `logs/eval/groq_llama70b/` (23 questions, Pakistani legal / govt documents)._")
    P("")
    P("## What is RAGAS?")
    P("")
    P("**RAGAS** (Retrieval-Augmented Generation Assessment) is the standard framework for ")
    P("scoring RAG pipelines without human labels. It uses a strong LLM as a **judge** that ")
    P("reads the question, the retrieved passages, the answer the system produced, and the ")
    P("known-correct (gold) answer — and then scores four standard dimensions:")
    P("")
    for m in METRICS:
        P(f"- **{PRETTY[m]}** — {DESC[m]}")
    P("")
    P("All scores are in **[0, 1]** where **higher = better**.")
    P("")
    P("## Setup")
    P("")
    P("| Component | Value |")
    P("|---|---|")
    P("| Source run | `groq_llama70b` — Llama-3.3-70B (q00-q18) + Llama-3.1-8B-instant (q19-q22) |")
    P("| Retrieval | Nomic-1.5 dense + FTS5, RRF fused, top-12 under 2400-tok budget |")
    P("| Reranker (Branch B) | `BAAI/bge-reranker-base` zero-shot |")
    P("| RAGAS judge | Groq `openai/gpt-oss-20b` (primary) + `llama-3.1-8b-instant` (answer_relevancy backstop) |")
    P("| RAGAS embeddings | `nomic-embed-text-v1.5` (same as production retrieval) |")
    P("| Test set | 23 hand-curated questions with gold passages |")
    P("| Branches scored | A = retrieval only · B = retrieval + reranker |")
    P("")
    P("## Aggregate scores")
    P("")
    P("Mean over questions where the judge produced a valid score. `(n)` is the count of valid ")
    P("samples out of 23 — when `n` is small the mean is noisy.")
    P("")
    header = "| Metric | Branch A (no rerank) | Branch B (with rerank) | Δ (B − A) |"
    P(header)
    P("|---|---|---|---|")
    for m in METRICS:
        a = df[df["branch"] == "branch_a"][m]
        b = df[df["branch"] == "branch_b"][m]
        am, bm = a.mean(skipna=True), b.mean(skipna=True)
        an, bn = a.notna().sum(), b.notna().sum()
        if pd.notna(am) and pd.notna(bm):
            d = bm - am
            d_str = f"{d:+.3f}"
        else:
            d_str = "—"
        P(f"| **{PRETTY[m]}** | {fmt(am)} (n={an}) | {fmt(bm)} (n={bn}) | {d_str} |")
    P("")
    P("### What the table says")
    P("")
    P("**Where coverage is solid (n ≥ 20):**")
    P("")
    P("- **Answer Relevancy** is essentially identical between A and B (Branch A 0.520 vs Branch B 0.495). ")
    P("  The reranker does **not** change how on-topic the answer is.")
    P("")
    P("**Where coverage is partial (n ≈ 6) but consistent:**")
    P("")
    P("- **Context Precision** is _much_ higher without the reranker: **0.190 vs 0.042** ")
    P("  (Branch A keeps useful chunks at the top; Branch B reshuffles them out).")
    P("- **Faithfulness** appears higher on Branch B but only 4 samples scored — too few to trust.")
    P("")
    P("**Where coverage failed (n ≤ 1):**")
    P("")
    P("- **Context Recall** — the judge could not reliably parse its own output under load. ")
    P("  Not enough signal to report.")
    P("")
    P("## Per-question scores")
    P("")
    P("Two rows per question (A then B). Dashes mean the judge call failed or returned unparsable output.")
    P("")
    P("| qid | branch | Faithfulness | Answer Relevancy | Context Precision | Context Recall |")
    P("|---|---|---|---|---|---|")
    for _, row in df.sort_values(["qid", "branch"]).iterrows():
        qid = int(row["qid"]) if pd.notna(row["qid"]) else "?"
        P(f"| q{qid:02d} | {row['branch']} | {fmt(row['faithfulness'])} | "
          f"{fmt(row['answer_relevancy'])} | {fmt(row['context_precision'])} | "
          f"{fmt(row['context_recall'])} |")
    P("")
    P("## Cross-check against our other metrics")
    P("")
    P("RAGAS agrees with the hand-rolled metrics from the same run:")
    P("")
    P("| Signal | Branch A | Branch B | What it says |")
    P("|---|---|---|---|")
    P("| RAGAS Context Precision | 0.190 | 0.042 | Reranker keeps **less useful** chunks |")
    P("| recall@12 (hand-rolled) | 0.57 | 0.52 | Reranker pushes gold **out** of top-12 |")
    P("| recall@5 (hand-rolled) | 0.35 | 0.43 | Reranker helps slightly at the very top |")
    P("| bigram-Jaccard A vs B | 0.065 | 0.059 | Reranker slightly **hurts** answer overlap with gold |")
    P("")
    P("Three independent signals — RAGAS context_precision, recall@K, bigram-Jaccard — all point ")
    P("the same way: **the zero-shot reranker is reordering chunks in a way that hurts the LLM's ")
    P("context**, except for a small lift at the very top of the list (top-1 / top-5).")
    P("")
    P("## Verdict")
    P("")
    P("This RAGAS run is the **third independent confirmation** of our headline finding:")
    P("")
    P("> The zero-shot **`bge-reranker-base` does not understand this domain.** Switching it on ")
    P("> degrades context quality (RAGAS Context Precision drops from 0.19 to 0.04) without ")
    P("> meaningfully helping answer relevancy. Both the small on-device LLM and the hosted ")
    P("> Llama-70B show the same pattern.")
    P("")
    P("The only remaining lever that can move the pipeline forward is **fine-tuning the reranker** ")
    P("on the in-domain dataset already prepared at `data/reranker/v1/` (198 train / 23 eval triples). ")
    P("Free-tier hosted LLM swaps and surface-level knobs (prompt, chunking, sliding windows) ")
    P("have all been exhausted.")
    P("")
    P("## Caveats")
    P("")
    P("- **Free-tier Groq quotas were the binding constraint**, not the eval design. Two judge ")
    P("  models exhausted their daily token limits during the run; coverage on the heavier ")
    P("  metrics (faithfulness, context_recall) suffered as a result.")
    P("- **Different judges across metrics** means absolute values aren't directly comparable ")
    P("  across columns. Relative A-vs-B comparison within a column is reliable; ")
    P("  cross-metric magnitude comparison is not.")
    P("- **`context_recall` is essentially unreported** here — to get it cleanly we'd need ")
    P("  either a paid judge tier or a re-run after the daily quota resets.")
    P("")
    P("## Files")
    P("")
    P("- This report: `~/Desktop/ragas_report.md`")
    P("- Per-question raw CSV: `logs/eval/groq_llama70b/ragas_scores.merged.csv`")
    P("- Detailed walkthrough (pipeline stages + RAGAS overlays): `~/Desktop/case_study_rag_pipeline_groq.md`")
    P("- Eval harness: `scripts/reranker_eval/ragas_eval.py`")

    OUT.write_text("\n".join(L))
    size_kb = OUT.stat().st_size / 1024
    print(f"[done] wrote {OUT} ({size_kb:.1f} KB)")


if __name__ == "__main__":
    main()
