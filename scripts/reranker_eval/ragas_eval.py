"""RAGAS evaluation over the trace JSONs produced by run_eval / rerun_*.

Scores four standard RAGAS metrics per question, per branch (A and B):
  - faithfulness          (does the answer stick to the retrieved passages?)
  - answer_relevancy      (does the answer address the question?)
  - context_precision     (are the retrieved chunks actually relevant?)
  - context_recall        (did retrieval find what's needed vs the gold answer?)

Judge: Groq (default llama-3.1-8b-instant — higher RPM than 70B).
Embeddings: sentence-transformers/all-MiniLM-L6-v2 (local, no API calls).

Output:
  <run_dir>/ragas_scores.csv     — per-question rows for both branches
  <run_dir>/ragas_summary.md     — averaged table

Usage:
  python ragas_eval.py --run logs/eval/nomic_full
  python ragas_eval.py --run logs/eval/groq_llama70b --judge llama-3.1-8b-instant
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import re
import sqlite3
import sys
import time
import zipfile
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
DB_PATH = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
KEY_FILE = Path("/home/riaz/Desktop/groq_api_key.odt")


def read_groq_key() -> str:
    with zipfile.ZipFile(KEY_FILE) as zf:
        content = zf.read("content.xml").decode("utf-8", errors="ignore")
    m = re.search(r"gsk_[A-Za-z0-9]{20,}", content)
    if not m:
        raise SystemExit("could not find gsk_ token in odt")
    return m.group(0)


def fetch_chunks(con: sqlite3.Connection, ids: list[str]) -> list[str]:
    if not ids:
        return []
    placeholders = ",".join("?" * len(ids))
    rows = con.execute(
        f"SELECT id, content FROM chunks WHERE id IN ({placeholders})",
        ids,
    ).fetchall()
    by_id = {r["id"]: r["content"] for r in rows}
    return [by_id[i] for i in ids if i in by_id]


def build_samples(run_dir: Path, db: sqlite3.Connection) -> list[dict]:
    """One sample per (qid, branch). Skip branches with no answer."""
    samples = []
    for tf in sorted(run_dir.glob("q*.trace.json")):
        t = json.loads(tf.read_text())
        qid = t["id"]
        q = t["query"]
        gold = t["gold_passage"]
        for branch in ("branch_a", "branch_b"):
            stage = t["stages"].get(branch, {})
            answer = (stage.get("answer") or "").strip()
            if not answer:
                continue
            ids = stage.get("selected_chunk_ids") or []
            contexts = fetch_chunks(db, ids)
            if not contexts:
                continue
            samples.append({
                "qid": qid,
                "branch": branch,
                "question": q,
                "answer": answer,
                "contexts": contexts,
                "ground_truth": gold,
            })
    return samples


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True, help="logs/eval/<run_dir>")
    ap.add_argument("--judge", default="llama-3.1-8b-instant",
                    help="Groq model name for RAGAS judge (default: 8b-instant for RPM headroom)")
    ap.add_argument("--throttle", type=float, default=1.2,
                    help="seconds to sleep between LLM calls (RAGAS makes many)")
    args = ap.parse_args()

    run_dir = Path(args.run)
    if not run_dir.is_absolute():
        run_dir = ROOT / args.run
    if not run_dir.exists():
        sys.exit(f"no such run: {run_dir}")

    # Set up Groq judge via env (langchain-groq picks it up)
    os.environ["GROQ_API_KEY"] = read_groq_key()
    print(f"[init] judge model: {args.judge}", flush=True)

    from langchain_groq import ChatGroq
    from langchain_core.embeddings import Embeddings
    from fastembed import TextEmbedding
    from ragas import evaluate
    from ragas.llms import LangchainLLMWrapper
    from ragas.embeddings import LangchainEmbeddingsWrapper
    from ragas.metrics import (
        faithfulness,
        answer_relevancy,
        context_precision,
        context_recall,
    )
    from datasets import Dataset

    class NomicEmbeddings(Embeddings):
        """LangChain Embeddings wrapper around fastembed Nomic — matches what
        production uses, so RAGAS' answer_relevancy metric lives in the same
        semantic space as the system under test."""
        def __init__(self):
            cache = ROOT / ".fastembed_cache"
            self._m = TextEmbedding(
                model_name="nomic-ai/nomic-embed-text-v1.5",
                cache_dir=str(cache),
            )

        def embed_documents(self, texts: list[str]) -> list[list[float]]:
            return [v.tolist() for v in self._m.embed(list(texts))]

        def embed_query(self, text: str) -> list[float]:
            return next(iter(self._m.query_embed([text]))).tolist()

    # Build judge LLM with retry on 429 (langchain-groq handles it via max_retries)
    judge = ChatGroq(
        model=args.judge,
        temperature=0.0,
        max_retries=8,
        request_timeout=240,
        max_tokens=2048,
        model_kwargs={"top_p": 1.0},
    )
    judge_wrapped = LangchainLLMWrapper(judge)

    # Local Nomic embeddings — same as production (consistency)
    print("[init] loading nomic-ai/nomic-embed-text-v1.5 via fastembed (local)", flush=True)
    emb = NomicEmbeddings()
    emb_wrapped = LangchainEmbeddingsWrapper(emb)

    metrics = [faithfulness, answer_relevancy, context_precision, context_recall]
    for m in metrics:
        m.llm = judge_wrapped
        if hasattr(m, "embeddings"):
            m.embeddings = emb_wrapped

    # Build dataset
    con = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row
    samples = build_samples(run_dir, con)
    print(f"[data] {len(samples)} (q,branch) samples from {run_dir.name}", flush=True)
    if not samples:
        sys.exit("no samples")

    ds = Dataset.from_list([
        {
            "question": s["question"],
            "answer": s["answer"],
            "contexts": s["contexts"],
            "ground_truth": s["ground_truth"],
        }
        for s in samples
    ])

    # Sequential execution + long timeouts: Groq's 8b-instant judge times out
    # under concurrent load and large faithfulness prompts. RunConfig forces
    # one job at a time.
    from ragas.run_config import RunConfig
    rc = RunConfig(timeout=300, max_retries=8, max_wait=90, max_workers=4)

    t0 = time.time()
    result = evaluate(
        ds,
        metrics=metrics,
        llm=judge_wrapped,
        embeddings=emb_wrapped,
        raise_exceptions=False,
        run_config=rc,
    )
    elapsed = time.time() - t0
    print(f"[eval] done in {elapsed/60:.1f} min", flush=True)

    # Result -> per-row CSV + summary md
    df = result.to_pandas()
    csv_path = run_dir / "ragas_scores.csv"
    df.insert(0, "qid", [s["qid"] for s in samples])
    df.insert(1, "branch", [s["branch"] for s in samples])
    df.to_csv(csv_path, index=False)
    print(f"[csv]  wrote {csv_path}", flush=True)

    # Averaged table by branch
    metric_cols = [c for c in df.columns if c not in {"qid", "branch", "question",
        "answer", "contexts", "ground_truth", "user_input", "response", "retrieved_contexts", "reference"}]
    summary = []
    for branch in ("branch_a", "branch_b"):
        sub = df[df["branch"] == branch]
        row = {"branch": branch, "n": len(sub)}
        for c in metric_cols:
            row[c] = round(float(sub[c].mean(skipna=True)), 4) if len(sub) else None
        summary.append(row)

    md = [f"# RAGAS Eval — {run_dir.name}", ""]
    md.append(f"_Judge: `{args.judge}` (Groq) · Embeddings: `nomic-embed-text-v1.5` (local, same as production)_")
    md.append(f"_Samples: {len(samples)} · Wall time: {elapsed/60:.1f} min_")
    md.append("")
    md.append("## Averaged metrics by branch")
    md.append("")
    header = ["branch", "n", *metric_cols]
    md.append("| " + " | ".join(header) + " |")
    md.append("|" + "---|" * len(header))
    for row in summary:
        md.append("| " + " | ".join(str(row.get(h, "")) for h in header) + " |")
    md.append("")
    md.append("**Higher = better** for all four metrics (range 0..1).")
    md.append("")
    md.append("- **faithfulness** — fraction of answer claims that are supported by retrieved contexts.")
    md.append("- **answer_relevancy** — how well the answer addresses the question.")
    md.append("- **context_precision** — fraction of retrieved chunks that are actually useful.")
    md.append("- **context_recall** — fraction of ground-truth info that's covered by retrieved chunks.")

    out_md = run_dir / "ragas_summary.md"
    out_md.write_text("\n".join(md))
    print(f"[md]   wrote {out_md}", flush=True)


if __name__ == "__main__":
    main()
