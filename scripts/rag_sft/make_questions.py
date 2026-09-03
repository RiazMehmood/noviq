#!/usr/bin/env python3
"""
Stage 1 (+4) — Question sourcing (Feature 019).

Builds questions.jsonl from up to three sources + a built-in adversarial set:
  --query-log-csv   : real past user queries (one column of text)   -> source_q=query_log
  --provisions-csv  : columns `instrument,unit_path` from `chunks`   -> source_q=provision
  --adversarial N   : N built-in out-of-corpus questions             -> source_q=adversarial

Produce the CSVs from prod PG (port drifts — `ss -tlnp|grep postgres`), e.g.:

  PGPASSWORD=... psql -h 127.0.0.1 -p <PORT> -U postgres -d echo -F',' --no-align -t -c \
    "SELECT DISTINCT query_text FROM query_log WHERE query_text <> '' LIMIT 2000" > query_log.csv

  PGPASSWORD=... psql -h 127.0.0.1 -p <PORT> -U postgres -d echo -F',' --no-align -t -c \
    "SELECT s.file_name, c.unit_path FROM chunks c JOIN sources s ON s.id=c.source_id
     WHERE c.unit_path ~ '/' ORDER BY random() LIMIT 1500" > provisions.csv

Usage:
  python make_questions.py --out questions.jsonl \
      --query-log-csv query_log.csv --provisions-csv provisions.csv --adversarial 300
"""
import argparse
import csv
import json
import re

# Provision -> question templates. {label} = a human provision label (e.g. "Rule 16(1)(a)").
PROVISION_TEMPLATES = [
    "What does {label} provide?",
    "Explain {label}.",
    "Under what conditions does {label} apply?",
    "Quote {label} verbatim.",
    "What is the procedure under {label}?",
]

# Out-of-corpus adversarial questions -> must trigger the refusal template. Mix of
# other-jurisdiction, other-domain, and plausible-but-absent legal topics.
ADVERSARIAL = [
    "What is the capital of France?",
    "What is the penalty for tax evasion under the US Internal Revenue Code?",
    "How do I register a company under UK law?",
    "What is the GST rate in India?",
    "Explain the offside rule in football.",
    "What are the visa requirements for Schengen countries?",
    "What is the minimum wage in California?",
    "How does the Indian Constitution define secularism?",
    "What is the procedure for divorce under Hindu Marriage Act?",
    "What is the interest rate set by the US Federal Reserve?",
]


def label_from_unit_path(instrument, unit_path):
    """Turn `Rule_16/(1)/(a)/(ii)` into a human label like `Rule 16(1)(a)(ii)`."""
    parts = [p for p in unit_path.split("/") if p]
    if not parts:
        return None
    head = parts[0].replace("_", " ")          # "Rule_16" -> "Rule 16"
    subs = "".join(parts[1:])                    # "(1)(a)(ii)"
    return (head + subs).strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--query-log-csv")
    ap.add_argument("--provisions-csv")
    ap.add_argument("--adversarial", type=int, default=0)
    ap.add_argument("--per-provision", type=int, default=1, help="templates to emit per provision")
    args = ap.parse_args()

    seen = set()
    out = []

    def add(q, src):
        q = re.sub(r"\s+", " ", q).strip()
        k = q.lower()
        if q and k not in seen:
            seen.add(k)
            out.append({"question": q, "source_q": src})

    if args.query_log_csv:
        with open(args.query_log_csv, encoding="utf-8") as f:
            for row in csv.reader(f):
                if row and row[0].strip():
                    add(row[0], "query_log")

    if args.provisions_csv:
        import itertools
        with open(args.provisions_csv, encoding="utf-8") as f:
            for row in csv.reader(f):
                if len(row) < 2:
                    continue
                label = label_from_unit_path(row[0].strip(), row[1].strip())
                if not label:
                    continue
                for tpl in itertools.islice(PROVISION_TEMPLATES, args.per_provision):
                    add(tpl.format(label=label), "provision")

    if args.adversarial:
        # cycle the built-in list up to the requested count
        i = 0
        while sum(1 for r in out if r["source_q"] == "adversarial") < args.adversarial and ADVERSARIAL:
            add(ADVERSARIAL[i % len(ADVERSARIAL)] + ("" if i < len(ADVERSARIAL) else f" (variant {i})"), "adversarial")
            i += 1
            if i > args.adversarial * 3:
                break

    with open(args.out, "w", encoding="utf-8") as f:
        for r in out:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")

    by_src = {}
    for r in out:
        by_src[r["source_q"]] = by_src.get(r["source_q"], 0) + 1
    print(f"wrote {len(out)} questions -> {args.out}")
    print("by source:", json.dumps(by_src, indent=2))


if __name__ == "__main__":
    main()
