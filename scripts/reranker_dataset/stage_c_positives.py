# scripts/reranker_dataset/stage_c_positives.py
"""Stage C: build (query, positive_chunk_id) records from corrected_train + test results."""
import json
from pathlib import Path
from scripts.reranker_dataset.config import OUT_DIR, TEST_RESULTS_DIR
from scripts.reranker_dataset.utils import db

def _norm(q: str) -> str:
    return " ".join(q.lower().split())

def dedupe(rows: list[dict]) -> list[dict]:
    seen: set[tuple[str, str]] = set()
    out: list[dict] = []
    for r in rows:
        key = (_norm(r["question"]), r["source_chunk_id"])
        if key in seen:
            continue
        seen.add(key)
        out.append(r)
    return out

def load_corrected_train() -> list[dict]:
    p = OUT_DIR / "corrected_train.jsonl"
    rows: list[dict] = []
    with p.open("r", encoding="utf-8") as f:
        for line in f:
            obj = json.loads(line)
            rows.append({
                "question": obj["question"],
                "source_chunk_id": obj["source_chunk_id"],
                "origin": "corrected_train",
            })
    return rows

def load_test_results() -> list[dict]:
    """Scan TEST_RESULTS_DIR for prompts that pair query with known-good chunk_id.

    Expected fields in prompt frontmatter or body: `query`, `expected_chunk_id`.
    Any prompt missing either field is skipped.
    """
    rows: list[dict] = []
    if not TEST_RESULTS_DIR.exists():
        return rows
    for p in TEST_RESULTS_DIR.rglob("*.prompt.md"):
        text = p.read_text(encoding="utf-8", errors="ignore")
        query = _kv(text, "query")
        chunk_id = _kv(text, "expected_chunk_id")
        if query and chunk_id:
            rows.append({
                "question": query.strip(),
                "source_chunk_id": chunk_id.strip(),
                "origin": f"test:{p.name}",
            })
    return rows

def _kv(text: str, key: str) -> str | None:
    import re
    m = re.search(rf"^\s*{re.escape(key)}\s*[:=]\s*(.+)$", text, re.MULTILINE | re.IGNORECASE)
    return m.group(1).strip() if m else None

def main() -> int:
    all_chunks = {c["id"]: c for c in db.load_chunks()}
    positives = dedupe(load_corrected_train() + load_test_results())
    # Drop positives whose chunk no longer exists in DB
    positives = [r for r in positives if r["source_chunk_id"] in all_chunks]
    out_path = OUT_DIR / "positives.jsonl"
    with out_path.open("w", encoding="utf-8") as f:
        for r in positives:
            chunk = all_chunks[r["source_chunk_id"]]
            f.write(json.dumps({
                "query": r["question"],
                "positive_chunk_id": r["source_chunk_id"],
                "positive_text": chunk["content"],
                "source_doc": chunk["source_name"],
                "origin": r["origin"],
            }) + "\n")
    print(f"Wrote {out_path} ({len(positives)} positives)")
    return 0

if __name__ == "__main__":
    import sys
    sys.exit(main())
