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
