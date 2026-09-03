"""Run all acceptance checks from the spec §8."""
import json
import sys
from scripts.reranker_dataset.config import OUT_DIR

CHECKS = []


def check(name, cond, detail=""):
    CHECKS.append((name, cond))
    status = "PASS" if cond else "FAIL"
    print(f"[{status}] {name}" + (f" — {detail}" if detail else ""))


def main():
    train = OUT_DIR / "train.jsonl"
    ev = OUT_DIR / "eval.jsonl"
    corrected = OUT_DIR / "corrected_train.jsonl"
    report = OUT_DIR / "audit_report_train.md"
    card = OUT_DIR / "dataset_card.md"

    check("train.jsonl exists", train.exists())
    check("eval.jsonl exists", ev.exists())
    check("corrected_train.jsonl exists", corrected.exists())
    check("audit_report_train.md exists", report.exists())
    check("dataset_card.md exists", card.exists())

    if not (train.exists() and ev.exists()):
        return 1

    train_rows = [json.loads(l) for l in train.open()]
    eval_rows = [json.loads(l) for l in ev.open()]

    ok_shape = all(
        isinstance(r.get("query"), str)
        and isinstance(r.get("pos"), list) and len(r["pos"]) == 1
        and isinstance(r.get("neg"), list) and 3 <= len(r["neg"]) <= 7
        for r in train_rows + eval_rows
    )
    check("every example has 1 pos and 3-7 negs", ok_shape)

    if corrected.exists():
        corr_rows = [json.loads(l) for l in corrected.open()]
        ok_fields = all("source_chunk_id" in r and "source_doc" in r for r in corr_rows)
        check("corrected_train entries have source_chunk_id + source_doc", ok_fields)

    train_q = {r["query"] for r in train_rows}
    eval_q = {r["query"] for r in eval_rows}
    check("no train/eval leakage", not (train_q & eval_q))

    total_bytes = train.stat().st_size + ev.stat().st_size
    check("total dataset < 50 MB", total_bytes < 50 * 1024 * 1024,
          detail=f"{total_bytes/1024/1024:.2f} MB")

    failed = [n for n, ok in CHECKS if not ok]
    if failed:
        print(f"\n{len(failed)} checks failed", file=sys.stderr)
        return 1
    print("\nAll acceptance checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
