"""Build a plain-English side-by-side comparison report for sharing.

Reads the three completed runs (baseline v5.2, prompt_v3, knob1_sliding) and
emits one big markdown file showing, for each question:
  - question text + gold passage (in full)
  - for each variant (6 total: 3 runs × A/B branches): full chunks fed to LLM,
    the system prompt that was used, the LLM's answer.

Goal: a non-technical reader can flip through and see exactly what the app
saw and what it said, with no jargon.
"""
from __future__ import annotations

import json
import re
import sqlite3
from pathlib import Path

ROOT = Path("/media/riaz/New Volume/echo-AI")
DB = Path("/media/riaz/New Volume/home-overflow/com.echoai.app/echo.db")
OUT = ROOT / "logs/eval/COMPARISON.md"

RUNS = [
    ("Run 1 — Baseline prompt (v5.2)", "nomic_full",      "v5.2 baseline"),
    ("Run 2 — Few-shot prompt (v3)",   "prompt_v3",        "v3 few-shot"),
    ("Run 3 — Few-shot + sliding rerank", "knob1_sliding150", "v3 few-shot + sliding-window rerank"),
]

# Shortened summaries of the system prompts (the actual full text is in run_eval.py
# / rerun_llm.py — keep this readable for the audience).
PROMPT_BLURBS = {
    "v5.2 baseline": (
        "Strict 'cite every sentence' prompt. Tells the model to answer using only the passages, "
        "cite with [N], and reply with the canned 'not enough information' line if nothing fits."
    ),
    "v3 few-shot": (
        "Same rules, but with a worked example showing the exact format we want: quote text from "
        "the passage, end each sentence with a [N] marker, no extra commentary."
    ),
    "v3 few-shot + sliding-window rerank": (
        "Same prompt as Run 2. The reranker scoring is changed: each passage is split into "
        "150-word overlapping windows and the passage's score is the MAX over its windows. "
        "Removes the 'first 256 words only' truncation that affected 86% of pairs in earlier runs."
    ),
}


def norm(s: str) -> str:
    return re.sub(r"\s+", " ", s).strip()


def fetch_chunk_text(con: sqlite3.Connection, ids: list[str]) -> list[dict]:
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


def render_branch(label: str, trace: dict, branch: str, con: sqlite3.Connection) -> str:
    L = []
    P = L.append
    stage = trace["stages"][branch]
    ids = stage["selected_chunk_ids"]
    chunks = fetch_chunk_text(con, ids)
    gold_rank = (
        trace["gold_ranks"]["hybrid_pre_rerank"] if branch == "branch_a"
        else trace["gold_ranks"]["after_rerank"]
    )
    P(f"#### {label}")
    P("")
    gold_str = f"rank **{gold_rank}** in this branch's list" if gold_rank else "**not found** in this branch"
    P(f"_Gold passage was {gold_str}._")
    P(f"_LLM took {stage['elapsed_ms']/1000:.1f}s, produced {stage['completion_tokens']} tokens._")
    P("")
    P(f"**Passages fed to the LLM** ({len(chunks)} total):")
    P("")
    for i, c in enumerate(chunks, 1):
        page = f", page {c['page_number']}" if c.get("page_number") else ""
        P(f"<details><summary>[{i}] {c['file_name']}{page}</summary>")
        P("")
        P("```")
        P(norm(c["content"]))
        P("```")
        P("</details>")
        P("")
    P("**LLM's answer:**")
    P("")
    P("```")
    P(stage["answer"].strip() or "(empty)")
    P("```")
    P("")
    return "\n".join(L)


def main():
    con = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    con.row_factory = sqlite3.Row

    runs_traces: dict[str, dict[int, dict]] = {}
    for _, run_dir, _ in RUNS:
        d = ROOT / "logs/eval" / run_dir
        runs_traces[run_dir] = {
            json.loads(p.read_text())["id"]: json.loads(p.read_text())
            for p in sorted(d.glob("q*.trace.json"))
        }

    L = []
    P = L.append

    P("# Reranker A/B Eval — Side-by-Side Comparison")
    P("")
    P("This document shows, for each of 23 test questions about Pakistani legal / government")
    P("documents, exactly what passages our app retrieved, what prompt it used, and what answer")
    P("the small on-device LLM (fine-tuned Qwen2.5-0.5B) produced.")
    P("")
    P("Three variants are compared:")
    P("")
    for title, run_dir, label in RUNS:
        P(f"- **{title}** — _{PROMPT_BLURBS[label]}_")
    P("")
    P("Each variant has **two branches**:")
    P("- **Branch A** = retrieval only (dense + keyword + fusion), no reranker reordering.")
    P("- **Branch B** = same retrieval, but a reranker reorders the candidates before they go to the LLM.")
    P("")
    P("**Why we ran these:** to find out whether the reranker actually helps, and whether prompt")
    P("changes can make the small LLM produce verbatim grounded answers. Headline result:")
    P("the zero-shot reranker is a wash on this domain (helps a bit at top-1/5, hurts at top-12),")
    P("and the 0.5B LLM paraphrases regardless of prompt. Fine-tuning the reranker is the real next step.")
    P("")
    P("---")
    P("")

    qids = sorted(next(iter(runs_traces.values())).keys())
    for qid in qids:
        # Pick any run for question/gold (same across runs)
        any_t = next(iter(runs_traces.values()))[qid]
        P(f"## Question {qid:02d}")
        P("")
        P(f"**Question asked:** {any_t['query']}")
        P("")
        P("**The 'gold' (correct) passage we hoped to retrieve:**")
        P("")
        P("```")
        P(norm(any_t["gold_passage"])[:1500] + ("..." if len(any_t["gold_passage"]) > 1500 else ""))
        P("```")
        P("")
        for title, run_dir, label in RUNS:
            P(f"### {title}")
            P("")
            P(f"_System prompt: **{label}** — {PROMPT_BLURBS[label]}_")
            P("")
            t = runs_traces[run_dir][qid]
            P(render_branch("Branch A — no reranker", t, "branch_a", con))
            P(render_branch("Branch B — with reranker", t, "branch_b", con))
        P("---")
        P("")

    OUT.write_text("\n".join(L))
    size_kb = OUT.stat().st_size / 1024
    print(f"[done] wrote {OUT} ({size_kb:.0f} KB, {len(qids)} questions)")


if __name__ == "__main__":
    main()
