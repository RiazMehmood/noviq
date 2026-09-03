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
