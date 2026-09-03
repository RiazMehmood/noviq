# scripts/reranker_dataset/stage_b_audit.py
"""Stage B: audit + repair train(3).jsonl."""
import json
from pathlib import Path

from scripts.reranker_dataset.utils import db, grounding


def load_train_jsonl(path: Path) -> list[dict]:
    """Parse JSONL. Skip malformed lines. Require 'question' and 'answer' keys.

    The Qwen training set may also use 'instruction'/'output' or 'prompt'/'completion';
    map those to question/answer if present.
    """
    rows: list[dict] = []
    with path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            q = obj.get("question") or obj.get("instruction") or obj.get("prompt")
            a = obj.get("answer") or obj.get("output") or obj.get("completion") or obj.get("response")
            if q and a:
                rows.append({"question": str(q).strip(), "answer": str(a).strip()})
    return rows


_STOP = {
    "the", "and", "for", "are", "was", "has", "have", "been", "will", "can",
    "may", "how", "what", "who", "when", "where", "why", "does", "did",
    "that", "this", "with", "from", "its", "not", "any", "all", "also",
    "more", "such", "than", "then", "they", "their", "there", "which",
}

def _best_candidate_for_question(question: str, top_k: int = 5) -> list[dict]:
    """Combine FTS hit with dense — for now use FTS only (BM25); dense added in stage D."""
    import re
    # Extract only alphanumeric content words (length >= 4, skip stop words).
    # Limit to 5 keywords so FTS5 AND-match finds results (FTS5 requires ALL terms
    # to appear in a chunk, so long queries return 0 hits).
    words = re.findall(r"\b[a-zA-Z]{4,}\b", question)
    keywords = [w for w in words if w.lower() not in _STOP]
    # FTS5 ANDs all terms — cap at 3 keywords so a single chunk can satisfy the query.
    # If 3 keywords return no hits, fall back to 2 then 1.
    for n in (3, 2, 1):
        kw = keywords[:n]
        if not kw:
            continue
        hits = db.fts_search(" ".join(kw), limit=top_k)
        if hits:
            return hits
    return []


def _extract_repair(question: str, chunk_content: str) -> str:
    """Pick the sentence in `chunk_content` most relevant to `question`.
    Heuristic: highest token overlap between question content-words and sentence."""
    import re
    sentences = [s.strip() for s in re.split(r"(?<=[.!?])\s+", chunk_content) if s.strip()]
    if not sentences:
        return chunk_content.strip()
    q_tokens = grounding._tokens(question)
    scored = [(grounding.overlap_score(" ".join(q_tokens), s), s) for s in sentences]
    scored.sort(reverse=True)
    return scored[0][1]


def classify_entry(question: str, answer: str, candidates: list[dict]) -> dict:
    """Classify one Q&A entry.

    Returns dict with keys:
      status: 'kept' | 'repaired' | 'dropped'
      source_chunk_id, source_doc, original_answer, repaired_answer (if repaired), reason
    """
    if not candidates:
        return {
            "status": "dropped",
            "reason": "no candidate chunk found via FTS",
            "original_answer": answer,
        }

    best = max(candidates, key=lambda c: grounding.overlap_score(answer, c["content"]))
    if grounding.is_grounded(answer, best["content"]):
        return {
            "status": "kept",
            "source_chunk_id": best["id"],
            "source_doc": best["source_name"],
            "original_answer": answer,
            "reason": "answer grounded in retrieved chunk",
        }

    # Wrong answer — is the *question* answerable from any candidate?
    q_grounded = max(
        (grounding.overlap_score(question, c["content"]) for c in candidates),
        default=0.0,
    )
    if q_grounded >= 0.25:
        anchor = max(candidates, key=lambda c: grounding.overlap_score(question, c["content"]))
        repaired = _extract_repair(question, anchor["content"])
        return {
            "status": "repaired",
            "source_chunk_id": anchor["id"],
            "source_doc": anchor["source_name"],
            "original_answer": answer,
            "repaired_answer": repaired,
            "reason": "answer not grounded; rewrote from question-aligned sentence in source",
        }

    return {
        "status": "dropped",
        "reason": "question not answerable from any indexed chunk",
        "original_answer": answer,
    }


def main() -> int:
    from scripts.reranker_dataset.config import TRAIN_JSONL, OUT_DIR
    rows = load_train_jsonl(TRAIN_JSONL)
    print(f"Loaded {len(rows)} entries from {TRAIN_JSONL}")

    kept: list[dict] = []
    repaired: list[dict] = []
    dropped: list[dict] = []
    for r in rows:
        candidates = _best_candidate_for_question(r["question"], top_k=5)
        result = classify_entry(r["question"], r["answer"], candidates)
        result["question"] = r["question"]
        if result["status"] == "kept":
            kept.append(result)
        elif result["status"] == "repaired":
            repaired.append(result)
        else:
            dropped.append(result)

    corrected_path = OUT_DIR / "corrected_train.jsonl"
    with corrected_path.open("w", encoding="utf-8") as f:
        import json
        for r in kept + repaired:
            f.write(json.dumps({
                "question": r["question"],
                "answer": r.get("repaired_answer", r["original_answer"]),
                "repaired": r["status"] == "repaired",
                "source_chunk_id": r["source_chunk_id"],
                "source_doc": r["source_doc"],
            }) + "\n")

    report_path = OUT_DIR / "audit_report_train.md"
    with report_path.open("w", encoding="utf-8") as f:
        f.write(f"# Audit report — train(3).jsonl\n\n")
        f.write(f"- Total loaded: **{len(rows)}**\n")
        f.write(f"- Kept (grounded): **{len(kept)}**\n")
        f.write(f"- Repaired: **{len(repaired)}**\n")
        f.write(f"- Dropped: **{len(dropped)}**\n\n")
        f.write("## Repaired entries\n\n")
        for r in repaired:
            f.write(f"### Q: {r['question']}\n\n")
            f.write(f"- **Original:** {r['original_answer']}\n")
            f.write(f"- **Repaired:** {r['repaired_answer']}\n")
            f.write(f"- **Source:** `{r['source_doc']}` chunk `{r['source_chunk_id']}`\n")
            f.write(f"- **Reason:** {r['reason']}\n\n")
        f.write("## Dropped entries\n\n")
        for r in dropped:
            f.write(f"- Q: {r['question']}\n  - Original A: {r['original_answer']}\n  - Reason: {r['reason']}\n\n")

    print(f"Wrote {corrected_path} ({len(kept) + len(repaired)} entries)")
    print(f"Wrote {report_path}")
    return 0

if __name__ == "__main__":
    import sys
    sys.exit(main())
