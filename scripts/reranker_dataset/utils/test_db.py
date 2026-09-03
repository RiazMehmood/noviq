# scripts/reranker_dataset/utils/test_db.py
import pytest
from scripts.reranker_dataset.utils import db

def test_load_chunks_returns_constitution_chunks():
    chunks = db.load_chunks()
    assert len(chunks) >= 1
    sample = chunks[0]
    assert set(sample.keys()) >= {"id", "source_id", "content", "source_name"}
    assert any("constitution" in c["source_name"].lower() for c in chunks)
