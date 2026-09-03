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
