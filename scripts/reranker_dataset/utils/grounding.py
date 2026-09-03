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
