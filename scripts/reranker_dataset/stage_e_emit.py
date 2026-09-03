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


def split(rows, train_frac, seed):
    rng = random.Random(seed)
    shuffled = list(rows)
    rng.shuffle(shuffled)
    cut = int(len(shuffled) * train_frac)
    return shuffled[:cut], shuffled[cut:]


def to_bge(raw):
    return {
        "query": raw["query"],
        "pos": [raw["positive_text"]],
        "neg": [n["text"] for n in raw["negatives"]],
    }


def _truncate(text, max_tokens):
    toks = text.split()
    if len(toks) <= max_tokens:
        return text, False
    return " ".join(toks[:max_tokens]), True


def _file_hash(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()[:16]


def main():
    raw_path = OUT_DIR / "triples_raw.jsonl"
    with raw_path.open("r", encoding="utf-8") as f:
        raw = [json.loads(line) for line in f]

    if len(raw) < MIN_TOTAL_POSITIVES:
        print(f"ERROR: only {len(raw)} positives, below floor {MIN_TOTAL_POSITIVES}. Halting.")
        return 2

    truncations = 0
    for r in raw:
        r["positive_text"], t = _truncate(r["positive_text"], MAX_TOKENS)
        truncations += int(t)
        for n in r["negatives"]:
            n["text"], t = _truncate(n["text"], MAX_TOKENS)
            truncations += int(t)

    train_raw, eval_raw = split(raw, SPLIT_TRAIN_FRAC, SEED)

    def _write(rows, path):
        with path.open("w", encoding="utf-8") as f:
            for r in rows:
                f.write(json.dumps(to_bge(r), ensure_ascii=False) + "\n")
        return path.stat().st_size

    train_path = OUT_DIR / "train.jsonl"
    eval_path = OUT_DIR / "eval.jsonl"
    train_bytes = _write(train_raw, train_path)
    eval_bytes = _write(eval_raw, eval_path)
    total_mb = (train_bytes + eval_bytes) / 1024 / 1024

    train_q = {r["query"] for r in train_raw}
    eval_q = {r["query"] for r in eval_raw}
    leakage = train_q & eval_q

    card_path = OUT_DIR / "dataset_card.md"
    with card_path.open("w", encoding="utf-8") as f:
        f.write("# Reranker dataset v1 — card\n\n")
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
