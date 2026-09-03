"""Merge two RAGAS runs by taking the non-null value per (qid,branch,metric).

Run 1 (parallel Llama-8B): high coverage on answer_relevancy, near-zero on others.
Run 2 (gpt-oss-20b sequential-ish): partial faithfulness + context_precision.

We don't average across judges — different judges have different calibration.
Instead, prefer Run 2 (gpt-oss-20b, smarter judge) where present, fall back to
Run 1 only for metrics Run 2 has poor coverage on. In practice this means:
  - faithfulness, context_precision, context_recall: Run 2 only
  - answer_relevancy: Run 1 (much better coverage)
"""
from __future__ import annotations
import pandas as pd
from pathlib import Path

RUN = Path("/media/riaz/New Volume/echo-AI/logs/eval/groq_llama70b")
RUN1 = RUN / "ragas_scores.partial.csv"   # llama-8b parallel
RUN2 = RUN / "ragas_scores.csv"            # gpt-oss-20b

METRICS = ["faithfulness", "answer_relevancy", "context_precision", "context_recall"]

df1 = pd.read_csv(RUN1)
df2 = pd.read_csv(RUN2)

key = ["qid", "branch"]
merged = df2[key + METRICS].copy()
merged = merged.merge(
    df1[key + ["answer_relevancy"]].rename(columns={"answer_relevancy": "ar_r1"}),
    on=key, how="outer",
)

# Coalesce: prefer Run 2 answer_relevancy if present, else Run 1
merged["answer_relevancy"] = merged["answer_relevancy"].fillna(merged["ar_r1"])
merged = merged.drop(columns=["ar_r1"])
merged = merged.sort_values(key).reset_index(drop=True)

out_csv = RUN / "ragas_scores.merged.csv"
merged.to_csv(out_csv, index=False)

# Summary
md = ["# RAGAS Merged Summary — groq_llama70b", ""]
md.append("_Best-coverage merge of two RAGAS runs:_")
md.append("- Run 1 (Llama-3.1-8B-instant, parallel) — used for `answer_relevancy` (high coverage)")
md.append("- Run 2 (openai/gpt-oss-20b, sequential) — used for `faithfulness`, `context_precision`, `context_recall`")
md.append("")
md.append("_Embeddings throughout: `nomic-embed-text-v1.5` (same as production)._")
md.append("")
md.append("## Per-branch averages")
md.append("")
md.append("| branch | n | " + " | ".join(f"{m} (n)" for m in METRICS) + " |")
md.append("|" + "---|" * (2 + len(METRICS)))
for branch in ("branch_a", "branch_b"):
    sub = merged[merged["branch"] == branch]
    row = [branch, str(len(sub))]
    for m in METRICS:
        nn = sub[m].notna().sum()
        mean = sub[m].mean(skipna=True)
        mean_str = f"{mean:.3f}" if pd.notna(mean) else "—"
        row.append(f"{mean_str} ({nn})")
    md.append("| " + " | ".join(row) + " |")
md.append("")
md.append("**Higher = better** (0..1). `(n)` is the sample count with a valid score.")
md.append("")
md.append("### What each metric means")
md.append("")
md.append("- **faithfulness** — fraction of answer claims supported by retrieved contexts (no hallucination).")
md.append("- **answer_relevancy** — how well the answer addresses the question.")
md.append("- **context_precision** — fraction of retrieved chunks that are actually useful.")
md.append("- **context_recall** — fraction of ground-truth info covered by retrieved chunks.")
md.append("")
md.append("### Caveats")
md.append("")
md.append("- Coverage for `context_recall` is near-zero (judge struggled to parse outputs / hit rate limits).")
md.append("- Different judges across metrics → not directly comparable in absolute terms.")
md.append("- Free-tier Groq quota is the constraint, not the eval design.")

(RUN / "ragas_summary.merged.md").write_text("\n".join(md))
print(f"[merged] {out_csv}")
print(f"[summary] {RUN / 'ragas_summary.merged.md'}")
