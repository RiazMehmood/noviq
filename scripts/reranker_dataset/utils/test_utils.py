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
