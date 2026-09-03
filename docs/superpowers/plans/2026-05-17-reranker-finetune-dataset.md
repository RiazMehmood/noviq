# Reranker Fine-Tuning Dataset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a clean BGE-format `(query, pos, negs)` triple dataset under `data/reranker/v1/` for fine-tuning `bge-reranker-base` on Google Colab T4.

**Architecture:** Standalone Python pipeline under `scripts/reranker_dataset/`. Stage A uses the running EchoAI app to index 6 docs. Stages B–E read `echo.db` directly (SQLite + sentence-transformers for re-embedding), audit/repair the old Qwen training set, build positives, mine hard negatives via dense + BM25, and emit JSONL splits.

**Tech Stack:** Python 3.11, `sentence-transformers` (BGE embedder for mining), `rank-bm25`, `sqlite3` stdlib, `pypdf` (for source-text loading during audit), `numpy`. No changes to the Rust app code.

**Spec:** `docs/superpowers/specs/2026-05-17-reranker-finetune-dataset-design.md`

---

## File Structure

```
scripts/reranker_dataset/
├── README.md                       # how to run, in order
├── requirements.txt                # pinned Python deps
├── config.py                       # paths, constants, seed
├── stage_b_audit.py                # audit + repair train(3).jsonl
├── stage_c_positives.py            # build (query, positive_chunk) records
├── stage_d_mine.py                 # hard-negative mining
├── stage_e_emit.py                 # JSONL emission + split + dataset card
└── utils/
    ├── db.py                       # echo.db reader (sources, chunks, FTS)
    ├── embedder.py                 # BGE embedder wrapper
    ├── bm25.py                     # BM25 over chunks
    └── grounding.py                # answer-grounded-in-chunk verification

data/reranker/v1/               # outputs (created at run time)
├── train.jsonl
├── eval.jsonl
├── corrected_train.jsonl
├── audit_report_train.md
└── dataset_card.md
```

Each file has one responsibility; intermediate artifacts live next to the script that creates them and feed the next stage.

---

## Task 0: Workspace setup

**Files:**
- Create: `scripts/reranker_dataset/requirements.txt`
- Create: `scripts/reranker_dataset/README.md`
- Create: `scripts/reranker_dataset/config.py`

- [ ] **Step 1: Create `requirements.txt`**

```
sentence-transformers==3.0.1
rank-bm25==0.2.2
pypdf==4.3.1
python-docx==1.1.2
numpy==1.26.4
tqdm==4.66.4
```

- [ ] **Step 2: Create `README.md`**

```markdown
# Reranker dataset pipeline

Run in order:

1. Stage A (manual): Open EchoAI app, add folder `~/Desktop/Sindh Govt Documents/`, wait for `indexing_status=indexed` on all 6 sources.
2. `python -m scripts.reranker_dataset.stage_b_audit`
3. `python -m scripts.reranker_dataset.stage_c_positives`
4. `python -m scripts.reranker_dataset.stage_d_mine`
5. `python -m scripts.reranker_dataset.stage_e_emit`

Outputs land in `data/reranker/v1/`.

Setup:
    python -m venv .venv-reranker
    source .venv-reranker/bin/activate
    pip install -r scripts/reranker_dataset/requirements.txt
```

- [ ] **Step 3: Create `config.py`**

```python
"""Central config — paths, constants, seed. Override via env vars if needed."""
import os
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# Input paths
ECHO_DB = Path(os.environ.get(
    "ECHO_DB",
    Path.home() / ".local/share/com.echoai.app/echo.db",
))
SINDH_DOCS_DIR = Path(os.environ.get(
    "SINDH_DOCS_DIR",
    Path.home() / "Desktop/Sindh Govt Documents",
))
TRAIN_JSONL = Path(os.environ.get(
    "TRAIN_JSONL",
    Path.home() / "Desktop/fine tunning research & work/train(3).jsonl",
))
TEST_RESULTS_DIR = REPO_ROOT / "history/prompts"  # narrowed in stage_c

# Output paths
OUT_DIR = REPO_ROOT / "data/reranker/v1"
OUT_DIR.mkdir(parents=True, exist_ok=True)

# Constants
EMBED_MODEL = "BAAI/bge-base-en-v1.5"  # 768-d, matches echo.db chunk dim
RERANKER_TARGET = "BAAI/bge-reranker-base"
MAX_TOKENS = 512
NEGATIVES_PER_POSITIVE = 7
MIN_NEGATIVES_PER_POSITIVE = 3
SCORE_FLOOR = 0.30      # below = trivially unrelated
SCORE_CEILING = 0.95    # above = near-duplicate, likely false negative
MINING_TOP_K = 15
SPLIT_TRAIN_FRAC = 0.90
SEED = 20260517
MIN_TOTAL_POSITIVES = 50  # halt if dataset would be too small

# Audit
AUDIT_OVERLAP_THRESHOLD = 0.45  # Jaccard token overlap required for "grounded"
```

- [ ] **Step 4: Create venv and install**

Run:
```
cd "/media/riaz/New Volume/echo-AI"
python3 -m venv .venv-reranker
source .venv-reranker/bin/activate
pip install -r scripts/reranker_dataset/requirements.txt
```

Expected: successful install, no resolver errors.

- [ ] **Step 5: Commit**

```bash
git add scripts/reranker_dataset/requirements.txt scripts/reranker_dataset/README.md scripts/reranker_dataset/config.py
git commit -m "chore(reranker-dataset): scaffold workspace, deps, config"
```

---

## Task 1: DB reader utility

**Files:**
- Create: `scripts/reranker_dataset/utils/__init__.py` (empty)
- Create: `scripts/reranker_dataset/utils/db.py`
- Create: `scripts/reranker_dataset/utils/test_db.py`

- [ ] **Step 1: Write failing test**

```python
# scripts/reranker_dataset/utils/test_db.py
import pytest
from scripts.reranker_dataset.utils import db

def test_load_chunks_returns_constitution_chunks():
    chunks = db.load_chunks()
    assert len(chunks) >= 1
    sample = chunks[0]
    assert set(sample.keys()) >= {"id", "source_id", "content", "source_name"}
    assert any("constitution" in c["source_name"].lower() for c in chunks)
```

- [ ] **Step 2: Run test, see ImportError**

Run: `pytest scripts/reranker_dataset/utils/test_db.py -v`
Expected: FAIL — `utils.db` does not exist yet.

- [ ] **Step 3: Implement `db.py`**

```python
"""Read-only access to echo.db. Never opens for write."""
import sqlite3
from pathlib import Path
from scripts.reranker_dataset.config import ECHO_DB

def _conn() -> sqlite3.Connection:
    if not ECHO_DB.exists():
        raise FileNotFoundError(f"echo.db not found at {ECHO_DB}")
    c = sqlite3.connect(f"file:{ECHO_DB}?mode=ro", uri=True)
    c.row_factory = sqlite3.Row
    return c

def load_sources() -> list[dict]:
    with _conn() as c:
        rows = c.execute(
            "SELECT id, file_name, file_path, format, indexing_status "
            "FROM sources WHERE indexing_status = 'indexed'"
        ).fetchall()
    return [dict(r) for r in rows]

def load_chunks() -> list[dict]:
    """Return leaf chunks only (the units actually retrieved)."""
    with _conn() as c:
        rows = c.execute(
            "SELECT chunks.id, chunks.source_id, chunks.content, "
            "       chunks.chunk_index, chunks.page_number, "
            "       sources.file_name AS source_name "
            "FROM chunks JOIN sources ON sources.id = chunks.source_id "
            "WHERE chunks.unit_type = 'leaf' "
            "  AND sources.indexing_status = 'indexed'"
        ).fetchall()
    return [dict(r) for r in rows]

def fts_search(query: str, limit: int = 20) -> list[dict]:
    """BM25 via SQLite FTS5. Returns chunk dicts with bm25 score."""
    sanitized = " ".join(t for t in query.split() if t.isalnum() or "-" in t)
    if not sanitized:
        return []
    with _conn() as c:
        rows = c.execute(
            "SELECT chunks.id, chunks.source_id, chunks.content, "
            "       sources.file_name AS source_name, "
            "       bm25(chunks_fts) AS score "
            "FROM chunks_fts "
            "JOIN chunks ON chunks.rowid = chunks_fts.rowid "
            "JOIN sources ON sources.id = chunks.source_id "
            "WHERE chunks_fts MATCH ? AND chunks.unit_type = 'leaf' "
            "ORDER BY score LIMIT ?",
            (sanitized, limit),
        ).fetchall()
    return [dict(r) for r in rows]
```

- [ ] **Step 4: Run test, see pass**

Run: `pytest scripts/reranker_dataset/utils/test_db.py -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/reranker_dataset/utils/
git commit -m "feat(reranker-dataset): db reader for sources, chunks, FTS"
```

---

## Task 2: Embedder + BM25 + grounding utilities

**Files:**
- Create: `scripts/reranker_dataset/utils/embedder.py`
- Create: `scripts/reranker_dataset/utils/bm25.py`
- Create: `scripts/reranker_dataset/utils/grounding.py`
- Create: `scripts/reranker_dataset/utils/test_utils.py`

- [ ] **Step 1: Write failing tests**

```python
# scripts/reranker_dataset/utils/test_utils.py
import numpy as np
from scripts.reranker_dataset.utils import embedder, bm25, grounding

def test_embedder_returns_768_dim():
    e = embedder.Embedder()
    v = e.encode(["hello world"])
    assert v.shape == (1, 768)
    assert np.isfinite(v).all()

def test_bm25_ranks_matching_doc_first():
    docs = ["the fox runs", "completely unrelated text", "fox jumps over dog"]
    b = bm25.BM25Index(docs)
    top = b.top_k("fox", k=2)
    assert 0 in [i for i, _ in top] or 2 in [i for i, _ in top]

def test_grounded_true_when_answer_substring_of_chunk():
    chunk = "Article 5 states that every citizen has the right to vote."
    answer = "every citizen has the right to vote"
    assert grounding.is_grounded(answer, chunk) is True

def test_grounded_false_when_answer_unrelated():
    chunk = "Article 5 states that every citizen has the right to vote."
    answer = "the sky is blue and water is wet"
    assert grounding.is_grounded(answer, chunk) is False
```

- [ ] **Step 2: Run tests, expect ImportError**

Run: `pytest scripts/reranker_dataset/utils/test_utils.py -v`
Expected: FAIL — modules not yet present.

- [ ] **Step 3: Implement `embedder.py`**

```python
"""Thin wrapper around sentence-transformers BGE encoder."""
from functools import lru_cache
import numpy as np
from sentence_transformers import SentenceTransformer
from scripts.reranker_dataset.config import EMBED_MODEL

class Embedder:
    def __init__(self, model_name: str = EMBED_MODEL):
        self._m = SentenceTransformer(model_name)

    def encode(self, texts: list[str]) -> np.ndarray:
        return self._m.encode(
            texts,
            convert_to_numpy=True,
            normalize_embeddings=True,
            show_progress_bar=False,
        )
```

- [ ] **Step 4: Implement `bm25.py`**

```python
"""BM25 over an in-memory list of documents."""
from rank_bm25 import BM25Okapi

class BM25Index:
    def __init__(self, docs: list[str]):
        self._tokens = [d.lower().split() for d in docs]
        self._bm25 = BM25Okapi(self._tokens)

    def top_k(self, query: str, k: int = 20) -> list[tuple[int, float]]:
        scores = self._bm25.get_scores(query.lower().split())
        ranked = sorted(enumerate(scores), key=lambda x: x[1], reverse=True)
        return ranked[:k]
```

- [ ] **Step 5: Implement `grounding.py`**

```python
"""Token-overlap grounding check. Conservative: prefer false negatives over false positives."""
import re
from scripts.reranker_dataset.config import AUDIT_OVERLAP_THRESHOLD

_WORD = re.compile(r"\w+")

def _tokens(s: str) -> set[str]:
    return {t.lower() for t in _WORD.findall(s) if len(t) > 2}

def overlap_score(answer: str, chunk: str) -> float:
    """Jaccard-like: |A ∩ C| / |A|. Asymmetric on purpose — we ask whether
    the answer's content words all appear in the chunk."""
    a = _tokens(answer)
    if not a:
        return 0.0
    c = _tokens(chunk)
    return len(a & c) / len(a)

def is_grounded(answer: str, chunk: str, threshold: float = AUDIT_OVERLAP_THRESHOLD) -> bool:
    return overlap_score(answer, chunk) >= threshold
```

- [ ] **Step 6: Run tests, expect PASS**

Run: `pytest scripts/reranker_dataset/utils/test_utils.py -v`
Expected: PASS on all 4 tests. The embedder test downloads BGE on first run (~430 MB, one-time).

- [ ] **Step 7: Commit**

```bash
git add scripts/reranker_dataset/utils/
git commit -m "feat(reranker-dataset): embedder, BM25, grounding utilities"
```

---

## Task 3: Stage A — Index Sindh govt docs (manual + verification)

**Files:**
- Create: `scripts/reranker_dataset/stage_a_verify.py`

Stage A indexing itself runs through the EchoAI app (the indexer is a Tauri service, not exposed as a CLI). This task scripts the **post-condition verification**.

- [ ] **Step 1: Implement verification script**

```python
"""Verify Stage A — confirm all 6 Sindh govt docs are indexed."""
import sys
from scripts.reranker_dataset.config import SINDH_DOCS_DIR
from scripts.reranker_dataset.utils import db

EXPECTED_FORMATS = {".pdf", ".docx", ".txt", ".md"}

def main() -> int:
    if not SINDH_DOCS_DIR.exists():
        print(f"ERROR: {SINDH_DOCS_DIR} does not exist", file=sys.stderr)
        return 2
    expected = [
        p for p in SINDH_DOCS_DIR.iterdir()
        if p.is_file() and p.suffix.lower() in EXPECTED_FORMATS
    ]
    expected_names = {p.name for p in expected}
    print(f"Expecting {len(expected_names)} indexed docs: {sorted(expected_names)}")

    sources = db.load_sources()
    indexed_names = {s["file_name"] for s in sources}
    missing = expected_names - indexed_names
    if missing:
        print(f"NOT YET INDEXED: {sorted(missing)}")
        print("Open EchoAI app, add folder, wait for indexing to complete.")
        return 1

    chunks = db.load_chunks()
    by_src: dict[str, int] = {}
    for c in chunks:
        by_src[c["source_name"]] = by_src.get(c["source_name"], 0) + 1
    print("\nIndexed chunk counts per source:")
    for name in sorted(expected_names):
        print(f"  {name}: {by_src.get(name, 0)} chunks")
    total = sum(by_src.get(n, 0) for n in expected_names)
    print(f"\nTotal Sindh-acts chunks: {total}")
    if total < 100:
        print("WARN: low chunk count — re-check indexing settings", file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Manual indexing step (user action)**

Action: Open EchoAI app. Use "Add Folder" to add `~/Desktop/Sindh Govt Documents/`. Wait until every file reaches `indexing_status='indexed'` in the source list.

- [ ] **Step 3: Run verification**

Run: `python -m scripts.reranker_dataset.stage_a_verify`
Expected: exit 0, prints chunk counts per source, total ≥ 100.

If it reports missing files, re-trigger indexing in the app, then re-run this step.

- [ ] **Step 4: Commit verification script**

```bash
git add scripts/reranker_dataset/stage_a_verify.py
git commit -m "feat(reranker-dataset): stage A indexing verification"
```

---

## Task 4: Stage B — Load + parse `train(3).jsonl`

**Files:**
- Create: `scripts/reranker_dataset/stage_b_audit.py`
- Create: `scripts/reranker_dataset/test_stage_b.py`

- [ ] **Step 1: Write a failing test for the loader**

```python
# scripts/reranker_dataset/test_stage_b.py
import json
import tempfile
from pathlib import Path
from scripts.reranker_dataset import stage_b_audit

def test_load_train_jsonl_parses_question_answer_pairs():
    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "t.jsonl"
        p.write_text('{"question":"q1","answer":"a1"}\n{"question":"q2","answer":"a2"}\n')
        rows = stage_b_audit.load_train_jsonl(p)
        assert len(rows) == 2
        assert rows[0]["question"] == "q1"
        assert rows[1]["answer"] == "a2"

def test_load_train_jsonl_skips_malformed_lines():
    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "t.jsonl"
        p.write_text('{"question":"q1","answer":"a1"}\nnot json\n{"question":"q2","answer":"a2"}\n')
        rows = stage_b_audit.load_train_jsonl(p)
        assert len(rows) == 2
```

- [ ] **Step 2: Run, expect ImportError**

Run: `pytest scripts/reranker_dataset/test_stage_b.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement loader (minimal)**

```python
# scripts/reranker_dataset/stage_b_audit.py
"""Stage B: audit + repair train(3).jsonl."""
import json
from pathlib import Path

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
            a = obj.get("answer") or obj.get("output") or obj.get("completion")
            if q and a:
                rows.append({"question": str(q).strip(), "answer": str(a).strip()})
    return rows
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `pytest scripts/reranker_dataset/test_stage_b.py -v`
Expected: PASS.

- [ ] **Step 5: Inspect real file structure** (one-shot exploration; no commit)

Run:
```
python -c "from scripts.reranker_dataset.stage_b_audit import load_train_jsonl; from scripts.reranker_dataset.config import TRAIN_JSONL; rows = load_train_jsonl(TRAIN_JSONL); print(f'rows={len(rows)}'); print(rows[0] if rows else 'EMPTY')"
```
Expected: prints row count and first row. If the format mapping in `load_train_jsonl` misses the actual keys, extend the `or`-chain in the function.

- [ ] **Step 6: Commit loader**

```bash
git add scripts/reranker_dataset/stage_b_audit.py scripts/reranker_dataset/test_stage_b.py
git commit -m "feat(reranker-dataset): stage B JSONL loader"
```

---

## Task 5: Stage B — Audit + repair pipeline

**Files:**
- Modify: `scripts/reranker_dataset/stage_b_audit.py`
- Modify: `scripts/reranker_dataset/test_stage_b.py`

- [ ] **Step 1: Write a failing test for audit classification**

```python
# scripts/reranker_dataset/test_stage_b.py — append
def test_classify_grounded_returns_kept():
    chunk = {"id": "c1", "content": "Article 5 grants every citizen the right to vote.", "source_name": "doc.pdf"}
    result = stage_b_audit.classify_entry(
        question="What does Article 5 grant?",
        answer="every citizen the right to vote",
        candidates=[chunk],
    )
    assert result["status"] == "kept"
    assert result["source_chunk_id"] == "c1"

def test_classify_wrong_but_repairable_returns_repaired():
    chunk = {"id": "c1", "content": "Article 5 grants every citizen the right to vote.", "source_name": "doc.pdf"}
    result = stage_b_audit.classify_entry(
        question="What does Article 5 grant?",
        answer="freedom of speech and assembly",  # wrong; correct text is in chunk
        candidates=[chunk],
    )
    assert result["status"] == "repaired"
    assert result["source_chunk_id"] == "c1"
    assert "vote" in result["repaired_answer"].lower()

def test_classify_unanswerable_returns_dropped():
    chunk = {"id": "c1", "content": "Article 5 grants every citizen the right to vote.", "source_name": "doc.pdf"}
    result = stage_b_audit.classify_entry(
        question="What is the boiling point of mercury?",
        answer="356.7 degrees Celsius",
        candidates=[chunk],
    )
    assert result["status"] == "dropped"
```

- [ ] **Step 2: Run tests, expect failures**

Run: `pytest scripts/reranker_dataset/test_stage_b.py -v`
Expected: FAIL — `classify_entry` not implemented.

- [ ] **Step 3: Implement classification + repair**

Add to `scripts/reranker_dataset/stage_b_audit.py`:

```python
from scripts.reranker_dataset.utils import db, grounding

def _best_candidate_for_question(question: str, top_k: int = 5) -> list[dict]:
    """Combine FTS hit with dense — for now use FTS only (BM25); dense added in stage D."""
    hits = db.fts_search(question, limit=top_k)
    return hits

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
    if q_grounded >= 0.30:
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
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `pytest scripts/reranker_dataset/test_stage_b.py -v`
Expected: PASS on all classification tests.

- [ ] **Step 5: Add the main runner**

Append to `stage_b_audit.py`:

```python
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
```

- [ ] **Step 6: Run the stage**

Run: `python -m scripts.reranker_dataset.stage_b_audit`
Expected: prints counts, writes `data/reranker/v1/corrected_train.jsonl` and `audit_report_train.md`. Open the audit report and skim repaired entries to confirm they look reasonable.

- [ ] **Step 7: Commit**

```bash
git add scripts/reranker_dataset/stage_b_audit.py scripts/reranker_dataset/test_stage_b.py
git commit -m "feat(reranker-dataset): stage B audit + repair + report"
```

---

## Task 6: Stage C — Build positives

**Files:**
- Create: `scripts/reranker_dataset/stage_c_positives.py`
- Create: `scripts/reranker_dataset/test_stage_c.py`

- [ ] **Step 1: Write a failing test**

```python
# scripts/reranker_dataset/test_stage_c.py
import json, tempfile
from pathlib import Path
from scripts.reranker_dataset import stage_c_positives

def test_dedupe_by_query_and_chunk():
    rows = [
        {"question": "Q1", "source_chunk_id": "c1"},
        {"question": "Q1", "source_chunk_id": "c1"},  # dup
        {"question": "Q1", "source_chunk_id": "c2"},  # same Q diff chunk → keep
        {"question": " q1 ", "source_chunk_id": "c1"},  # normalized dup
    ]
    out = stage_c_positives.dedupe(rows)
    assert len(out) == 2
```

- [ ] **Step 2: Run, expect ImportError**

Run: `pytest scripts/reranker_dataset/test_stage_c.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement Stage C**

```python
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
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `pytest scripts/reranker_dataset/test_stage_c.py -v`
Expected: PASS.

- [ ] **Step 5: Run the stage**

Run: `python -m scripts.reranker_dataset.stage_c_positives`
Expected: prints positive count, writes `data/reranker/v1/positives.jsonl`. If the count is < 50, surface to user — likely test-result discovery is missing.

- [ ] **Step 6: Commit**

```bash
git add scripts/reranker_dataset/stage_c_positives.py scripts/reranker_dataset/test_stage_c.py
git commit -m "feat(reranker-dataset): stage C positives builder"
```

---

## Task 7: Stage D — Hard-negative mining

**Files:**
- Create: `scripts/reranker_dataset/stage_d_mine.py`
- Create: `scripts/reranker_dataset/test_stage_d.py`

- [ ] **Step 1: Write a failing test for filtering logic**

```python
# scripts/reranker_dataset/test_stage_d.py
from scripts.reranker_dataset import stage_d_mine

def test_filter_within_score_band_drops_extremes():
    candidates = [
        ("c1", 0.99),  # too high → false negative
        ("c2", 0.80),  # good
        ("c3", 0.50),  # good
        ("c4", 0.20),  # too low → useless
    ]
    kept = stage_d_mine.filter_score_band(candidates, floor=0.30, ceiling=0.95)
    ids = [c for c, _ in kept]
    assert ids == ["c2", "c3"]

def test_sample_caps_at_n():
    candidates = [(f"c{i}", 0.6) for i in range(20)]
    out = stage_d_mine.sample_negatives(candidates, n=7, seed=42)
    assert len(out) == 7
```

- [ ] **Step 2: Run, expect ImportError**

Run: `pytest scripts/reranker_dataset/test_stage_d.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement Stage D**

```python
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
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `pytest scripts/reranker_dataset/test_stage_d.py -v`
Expected: PASS.

- [ ] **Step 5: Run the stage**

Run: `python -m scripts.reranker_dataset.stage_d_mine`
Expected: progress bar over positives, ends with line `Mined N triples (...)`. Writes `data/reranker/v1/triples_raw.jsonl`. First run downloads BGE (~430 MB) if not cached.

- [ ] **Step 6: Commit**

```bash
git add scripts/reranker_dataset/stage_d_mine.py scripts/reranker_dataset/test_stage_d.py
git commit -m "feat(reranker-dataset): stage D hard-negative mining"
```

---

## Task 8: Stage E — Emit BGE-format dataset + split + card

**Files:**
- Create: `scripts/reranker_dataset/stage_e_emit.py`
- Create: `scripts/reranker_dataset/test_stage_e.py`

- [ ] **Step 1: Write a failing test for splitting and format**

```python
# scripts/reranker_dataset/test_stage_e.py
from scripts.reranker_dataset import stage_e_emit

def test_split_is_reproducible():
    rows = [{"query": f"q{i}"} for i in range(100)]
    train1, eval1 = stage_e_emit.split(rows, train_frac=0.9, seed=42)
    train2, eval2 = stage_e_emit.split(rows, train_frac=0.9, seed=42)
    assert [r["query"] for r in train1] == [r["query"] for r in train2]
    assert [r["query"] for r in eval1] == [r["query"] for r in eval2]
    assert len(train1) == 90 and len(eval1) == 10

def test_split_has_no_leakage():
    rows = [{"query": f"q{i}"} for i in range(100)]
    train, ev = stage_e_emit.split(rows, train_frac=0.9, seed=42)
    train_q = {r["query"] for r in train}
    eval_q = {r["query"] for r in ev}
    assert train_q & eval_q == set()

def test_to_bge_format_emits_query_pos_neg_keys():
    raw = {
        "query": "what?",
        "positive_text": "the answer",
        "negatives": [{"text": "n1"}, {"text": "n2"}, {"text": "n3"}],
    }
    bge = stage_e_emit.to_bge(raw)
    assert set(bge.keys()) == {"query", "pos", "neg"}
    assert bge["pos"] == ["the answer"]
    assert bge["neg"] == ["n1", "n2", "n3"]
```

- [ ] **Step 2: Run, expect ImportError**

Run: `pytest scripts/reranker_dataset/test_stage_e.py -v`
Expected: FAIL.

- [ ] **Step 3: Implement Stage E**

```python
# scripts/reranker_dataset/stage_e_emit.py
"""Stage E: split, format as BGE FlagEmbedding JSONL, write dataset card."""
import json
import random
import hashlib
from pathlib import Path
from scripts.reranker_dataset.config import (
    OUT_DIR, SPLIT_TRAIN_FRAC, SEED, MAX_TOKENS, NEGATIVES_PER_POSITIVE,
    MIN_NEGATIVES_PER_POSITIVE, SCORE_FLOOR, SCORE_CEILING,
    EMBED_MODEL, RERANKER_TARGET, MIN_TOTAL_POSITIVES,
)

def split(rows: list[dict], train_frac: float, seed: int) -> tuple[list[dict], list[dict]]:
    rng = random.Random(seed)
    shuffled = list(rows)
    rng.shuffle(shuffled)
    cut = int(len(shuffled) * train_frac)
    return shuffled[:cut], shuffled[cut:]

def to_bge(raw: dict) -> dict:
    return {
        "query": raw["query"],
        "pos": [raw["positive_text"]],
        "neg": [n["text"] for n in raw["negatives"]],
    }

def _truncate(text: str, max_tokens: int) -> tuple[str, bool]:
    """Whitespace-token truncation (proxy for BGE tokenizer 512 cap)."""
    toks = text.split()
    if len(toks) <= max_tokens:
        return text, False
    return " ".join(toks[:max_tokens]), True

def _file_hash(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()[:16]

def main() -> int:
    raw_path = OUT_DIR / "triples_raw.jsonl"
    with raw_path.open("r", encoding="utf-8") as f:
        raw = [json.loads(line) for line in f]

    if len(raw) < MIN_TOTAL_POSITIVES:
        print(f"ERROR: only {len(raw)} positives, below floor {MIN_TOTAL_POSITIVES}. Halting.")
        return 2

    # Truncate long texts; record stats
    truncations = 0
    for r in raw:
        r["positive_text"], t = _truncate(r["positive_text"], MAX_TOKENS)
        truncations += int(t)
        for n in r["negatives"]:
            n["text"], t = _truncate(n["text"], MAX_TOKENS)
            truncations += int(t)

    train_raw, eval_raw = split(raw, SPLIT_TRAIN_FRAC, SEED)

    def _write(rows: list[dict], path: Path) -> int:
        with path.open("w", encoding="utf-8") as f:
            for r in rows:
                f.write(json.dumps(to_bge(r), ensure_ascii=False) + "\n")
        return path.stat().st_size

    train_path = OUT_DIR / "train.jsonl"
    eval_path = OUT_DIR / "eval.jsonl"
    train_bytes = _write(train_raw, train_path)
    eval_bytes = _write(eval_raw, eval_path)
    total_mb = (train_bytes + eval_bytes) / 1024 / 1024

    # Leakage check
    train_q = {r["query"] for r in train_raw}
    eval_q = {r["query"] for r in eval_raw}
    leakage = train_q & eval_q

    card_path = OUT_DIR / "dataset_card.md"
    with card_path.open("w", encoding="utf-8") as f:
        f.write("# Reranker dataset v1 — card\n\n")
        f.write(f"**Built:** {Path(__file__).name} on `{OUT_DIR.parent.parent}`\n\n")
        f.write(f"**Target model:** `{RERANKER_TARGET}`  \n")
        f.write(f"**Embedder used for mining:** `{EMBED_MODEL}`  \n")
        f.write(f"**Training environment:** Google Colab T4 (16 GB VRAM)\n\n")
        f.write("## Counts\n\n")
        f.write(f"- Raw triples: **{len(raw)}**\n")
        f.write(f"- Train: **{len(train_raw)}** ({train_bytes/1024:.1f} KB)\n")
        f.write(f"- Eval: **{len(eval_raw)}** ({eval_bytes/1024:.1f} KB)\n")
        f.write(f"- Total size: **{total_mb:.2f} MB**\n")
        f.write(f"- Truncations (>{MAX_TOKENS} tokens): **{truncations}**\n")
        f.write(f"- Train/eval query leakage: **{len(leakage)}** (must be 0)\n\n")
        f.write("## Mining settings\n\n")
        f.write(f"- negatives/positive: {NEGATIVES_PER_POSITIVE} (min {MIN_NEGATIVES_PER_POSITIVE})\n")
        f.write(f"- score band: [{SCORE_FLOOR}, {SCORE_CEILING}]\n")
        f.write(f"- seed: {SEED}\n\n")
        f.write("## Source files\n\n")
        for p in [OUT_DIR / "corrected_train.jsonl", OUT_DIR / "positives.jsonl", raw_path]:
            if p.exists():
                f.write(f"- `{p.name}` sha256:{_file_hash(p)}\n")
        f.write("\n## Known limitations\n\n")
        f.write("- Single domain (legal/govt). Broaden in v2.\n")
        f.write(f"- Truncation at {MAX_TOKENS} whitespace tokens approximates BGE tokenizer 512 cap.\n")

    print(f"Train: {len(train_raw)}  Eval: {len(eval_raw)}  Total: {total_mb:.2f} MB")
    print(f"Leakage: {len(leakage)}")
    print(f"Wrote {train_path}, {eval_path}, {card_path}")
    return 0 if not leakage else 1

if __name__ == "__main__":
    import sys
    sys.exit(main())
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `pytest scripts/reranker_dataset/test_stage_e.py -v`
Expected: PASS.

- [ ] **Step 5: Run the stage**

Run: `python -m scripts.reranker_dataset.stage_e_emit`
Expected: prints counts, leakage = 0, writes train/eval/card. If exit code 2, dataset too small — investigate stage C/D.

- [ ] **Step 6: Commit**

```bash
git add scripts/reranker_dataset/stage_e_emit.py scripts/reranker_dataset/test_stage_e.py
git commit -m "feat(reranker-dataset): stage E split, BGE format, dataset card"
```

---

## Task 9: Acceptance verification

**Files:**
- Create: `scripts/reranker_dataset/verify_acceptance.py`

- [ ] **Step 1: Implement acceptance script**

```python
"""Run all acceptance checks from the spec §8."""
import json
import sys
from pathlib import Path
from scripts.reranker_dataset.config import OUT_DIR

CHECKS: list[tuple[str, bool]] = []

def check(name: str, cond: bool, detail: str = "") -> None:
    CHECKS.append((name, cond))
    status = "PASS" if cond else "FAIL"
    print(f"[{status}] {name}" + (f" — {detail}" if detail else ""))

def main() -> int:
    train = OUT_DIR / "train.jsonl"
    ev = OUT_DIR / "eval.jsonl"
    corrected = OUT_DIR / "corrected_train.jsonl"
    report = OUT_DIR / "audit_report_train.md"
    card = OUT_DIR / "dataset_card.md"

    check("train.jsonl exists", train.exists())
    check("eval.jsonl exists", ev.exists())
    check("corrected_train.jsonl exists", corrected.exists())
    check("audit_report_train.md exists", report.exists())
    check("dataset_card.md exists", card.exists())

    if not (train.exists() and ev.exists()):
        return 1

    train_rows = [json.loads(l) for l in train.open()]
    eval_rows = [json.loads(l) for l in ev.open()]

    ok_shape = all(
        isinstance(r.get("query"), str)
        and isinstance(r.get("pos"), list) and len(r["pos"]) == 1
        and isinstance(r.get("neg"), list) and 3 <= len(r["neg"]) <= 7
        for r in train_rows + eval_rows
    )
    check("every example has 1 pos and 3-7 negs", ok_shape)

    if corrected.exists():
        corr_rows = [json.loads(l) for l in corrected.open()]
        ok_fields = all("source_chunk_id" in r and "source_doc" in r for r in corr_rows)
        check("corrected_train entries have source_chunk_id + source_doc", ok_fields)

    train_q = {r["query"] for r in train_rows}
    eval_q = {r["query"] for r in eval_rows}
    check("no train/eval leakage", not (train_q & eval_q))

    total_bytes = train.stat().st_size + ev.stat().st_size
    check("total dataset < 50 MB", total_bytes < 50 * 1024 * 1024,
          detail=f"{total_bytes/1024/1024:.2f} MB")

    failed = [n for n, ok in CHECKS if not ok]
    if failed:
        print(f"\n{len(failed)} checks failed", file=sys.stderr)
        return 1
    print("\nAll acceptance checks passed.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run**

Run: `python -m scripts.reranker_dataset.verify_acceptance`
Expected: every line PASS, final "All acceptance checks passed."

- [ ] **Step 3: Commit**

```bash
git add scripts/reranker_dataset/verify_acceptance.py
git commit -m "feat(reranker-dataset): acceptance verification script"
```

- [ ] **Step 4: Final dataset commit**

The dataset itself (under `data/reranker/v1/`) should be checked in only if size allows. Since the spec caps at < 50 MB, this is fine.

```bash
git add data/reranker/v1/
git commit -m "data(reranker): v1 dataset for bge-reranker-base fine-tune"
```

- [ ] **Step 5: Update CHANGELOG_DEV.md**

Append a dated entry per CLAUDE.md mandate. Symptom: low Tier-3 reranker quality (tests 5–8 = 0/15). Fix: this dataset enables a domain-targeted fine-tune. Cross-cutting impact: actual fine-tune is a separate workstream on Colab T4. Follow-ups: training notebook, eval against test queries, v2 dataset with broader domain mix.

---

## Self-Review

- **Spec coverage:** Stages A–E from spec §3 → Tasks 3, 4–5, 6, 7, 8 respectively. Acceptance checks §8 → Task 9. T4 constraints §5 → enforced via `MAX_TOKENS=512` and < 50 MB cap in Task 8. Scope guards §6 → no LLM synthetic queries, no re-chunking, no Rust changes — none of those appear in tasks. Risks §7 → mitigated: audit logged (Task 5), positives floor enforced (Task 8 main returns 2 if < 50), truncation counted (Task 8 card), stale chunk-ids dropped (Task 6 stage_c filters by existing IDs).
- **Placeholders:** none — every step has code or exact command.
- **Type consistency:** `source_chunk_id` used consistently across Tasks 5/6/9. `positive_chunk_id`/`positive_text` consistent across 6/7/8. `to_bge` keys `query`/`pos`/`neg` match BGE FlagEmbedding spec referenced in §3 Stage E.

---

## Risks during execution

- **Stage A is manual.** If the user adds the folder but indexing silently fails for some files, Task 3 verification will catch it. The retry path is reopen the app or use `retry_folder_source`.
- **`train(3).jsonl` field names may differ.** Task 4 Step 5 explicitly inspects the file before committing the loader. If neither `question`/`instruction`/`prompt` nor `answer`/`output`/`completion` works, the loader maps need a small extension.
- **Test results may not contain `expected_chunk_id`.** If `load_test_results` returns 0 rows, the dataset is built from `corrected_train.jsonl` alone — this still satisfies the spec's MIN_TOTAL_POSITIVES floor as long as `train(3).jsonl` is non-trivial.
