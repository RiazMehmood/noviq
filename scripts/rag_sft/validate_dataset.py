#!/usr/bin/env python3
"""
Stage 5 — Quality gates (Feature 019).

Rejects any record that would poison training. Reads raw.jsonl, writes clean.jsonl +
rejects.jsonl + a report. Optionally splits the clean set 90/5/5 (dedup by question).

Gates:
  1. citation-validity : every [N] in the answer exists in the Context block
  2. no-leakage        : no article/rule number in the answer absent from the context
  3. refusal-correct   : refusals actually refuse & have 0 cites; answers actually answer
  4. format-sanity     : non-empty; marker-density < 30% (not a [3][4]… dump)

Usage:
  python validate_dataset.py --in raw.jsonl --clean clean.jsonl \
      --rejects rejects.jsonl --report report.json [--split] [--out-dir data/]
"""
import argparse
import json
import re
import hashlib

import rag_sft_lib as lib

MARKER_RE = re.compile(r"\[(\d+)\]")
# legal number tokens like "Rule 16", "Article 25(1)", "Section 7A", "16(1)(a)"
LEGALNUM_RE = re.compile(r"\b(?:Rule|Article|Art|Section|Sec|Regulation)\s+\d+[A-Z]?(?:\([\dA-Za-z]+\))*", re.I)


def context_marker_set(user_content):
    # markers present in the Context block == the valid [N] range
    ctx = user_content.split("\n\nQuestion:")[0]
    return set(int(m) for m in MARKER_RE.findall(ctx))


def check(rec):
    """Return (ok: bool, reasons: list[str])."""
    reasons = []
    msgs = {m["role"]: m["content"] for m in rec.get("messages", [])}
    user = msgs.get("user", "")
    answer = (msgs.get("assistant", "") or "").strip()
    meta = rec.get("meta", {})
    kind = meta.get("kind", "answer")

    if not answer:
        return False, ["empty-answer"]

    valid = context_marker_set(user)
    used = set(int(m) for m in MARKER_RE.findall(answer))

    # 1. citation validity
    bad = used - valid
    if bad:
        reasons.append(f"invalid-citations:{sorted(bad)}")

    # 4. format sanity — marker density (chars that are [N] markers vs total)
    marker_chars = sum(len(m.group(0)) for m in MARKER_RE.finditer(answer))
    density = marker_chars / max(len(answer), 1)
    if density > 0.30:
        reasons.append(f"marker-dump:{density:.2f}")

    # 3. refusal correctness
    is_refusal_text = lib.REFUSAL_TEXT.lower() in answer.lower()
    if kind == "refusal":
        if not is_refusal_text:
            reasons.append("refusal-without-refusal-text")
        if used:
            reasons.append("refusal-with-citations")
    else:  # answer
        if is_refusal_text:
            reasons.append("answer-is-actually-refusal")
        if not used:
            reasons.append("answer-without-citations")

    # 2. no leakage — legal numbers in answer must appear somewhere in the context
    ctx = user.lower()
    for tok in LEGALNUM_RE.findall(answer):
        # normalize whitespace for the containment check
        norm = re.sub(r"\s+", " ", tok.lower())
        if norm not in re.sub(r"\s+", " ", ctx):
            reasons.append(f"leaked-legal-ref:{tok}")
            break

    return (len(reasons) == 0), reasons


def qkey(rec):
    user = next((m["content"] for m in rec["messages"] if m["role"] == "user"), "")
    q = user.split("Question:")[-1].strip().lower()
    return hashlib.sha1(q.encode("utf-8")).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--in", dest="inp", required=True)
    ap.add_argument("--clean", default="clean.jsonl")
    ap.add_argument("--rejects", default="rejects.jsonl")
    ap.add_argument("--report", default="report.json")
    ap.add_argument("--split", action="store_true")
    ap.add_argument("--out-dir", default=".")
    args = ap.parse_args()

    clean, rejects = [], []
    reason_counts, kind_counts = {}, {}
    seen = set()

    for rec in lib.read_jsonl(args.inp):
        k = qkey(rec)
        if k in seen:
            rejects.append({**rec, "_reject": ["dup-question"]})
            reason_counts["dup-question"] = reason_counts.get("dup-question", 0) + 1
            continue
        seen.add(k)
        ok, reasons = check(rec)
        if ok:
            clean.append(rec)
            kd = rec.get("meta", {}).get("kind", "answer")
            kind_counts[kd] = kind_counts.get(kd, 0) + 1
        else:
            rejects.append({**rec, "_reject": reasons})
            for r in reasons:
                key = r.split(":")[0]
                reason_counts[key] = reason_counts.get(key, 0) + 1

    lib.write_jsonl(args.clean, clean)
    lib.write_jsonl(args.rejects, rejects)

    total = len(clean) + len(rejects)
    report = {
        "total": total,
        "clean": len(clean),
        "rejected": len(rejects),
        "reject_rate": round(len(rejects) / max(total, 1), 3),
        "reject_reasons": dict(sorted(reason_counts.items(), key=lambda x: -x[1])),
        "clean_kind_mix": kind_counts,
    }
    with open(args.report, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)
    print(json.dumps(report, indent=2))

    if args.split and clean:
        import random
        random.seed(42)
        random.shuffle(clean)
        n = len(clean)
        n_test = max(1, n // 20)
        n_val = max(1, n // 20)
        test, val, train = clean[:n_test], clean[n_test:n_test + n_val], clean[n_test + n_val:]
        import os
        for name, part in (("train", train), ("val", val), ("test", test)):
            p = os.path.join(args.out_dir, f"{name}.jsonl")
            lib.write_jsonl(p, part)
            print(f"  {name}: {len(part)} -> {p}")


if __name__ == "__main__":
    main()
