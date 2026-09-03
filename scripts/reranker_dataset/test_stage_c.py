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
