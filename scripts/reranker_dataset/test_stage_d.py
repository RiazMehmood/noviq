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
