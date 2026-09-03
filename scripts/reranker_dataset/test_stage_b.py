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
