#!/usr/bin/env python3
"""Split the 20-prompt demo set into val (10) and held-out (10), stratified by tier.

Writes:
  data/val_prompts.jsonl       — 10 prompts, used for per-epoch eval during training
  data/heldout_prompts.jsonl   — 10 prompts, used ONLY for final post-training scoring
                                  (NEVER seen during training or in-training eval)

Tier-stratified split (2-3 per tier in each bucket).
"""

import json
from pathlib import Path

OUT_DIR = Path(__file__).resolve().parent / "data"
OUT_DIR.mkdir(parents=True, exist_ok=True)

PROMPTS = [
    # Tier 1 — definitional recall
    (1,  "T1", "What does the Constitution define as a 'Service of Pakistan'?"),
    (2,  "T1", "How is 'Majlis-e-Shoora (Parliament)' defined in the Constitution?"),
    (3,  "T1", "What is the constitutional definition of 'State' for the purposes of Fundamental Rights?"),
    (4,  "T1", "Quote the definition of 'Money Bill' as it appears in the Constitution."),
    (5,  "T1", "What does Article 260 say about the term 'citizen'?"),
    # Tier 2 — single-hop article lookup
    (6,  "T2", "What does Article 25 guarantee regarding equality of citizens?"),
    (7,  "T2", "Summarise Article 19 on freedom of speech, including its stated restrictions."),
    (8,  "T2", "What protections does Article 10A provide?"),
    (9,  "T2", "List the languages mentioned in Article 251."),
    (10, "T2", "What does Article 6 declare about high treason?"),
    # Tier 3 — structural / navigational
    (11, "T3", "Which Articles fall under Chapter 1 of Part II (Fundamental Rights)? List them with titles."),
    (12, "T3", "What are the Principles of Policy listed in Chapter 2 of Part II?"),
    (13, "T3", "How many schedules does the Constitution have, and what does the Fourth Schedule cover?"),
    (14, "T3", "Walk me through the composition of the National Assembly as set out in Article 51."),
    (15, "T3", "What is the procedure for the election of the President under Article 41?"),
    # Tier 4 — multi-hop reasoning
    (16, "T4", "Compare the qualifications for membership of the National Assembly (Article 62) with the disqualifications (Article 63). Where do they overlap?"),
    (17, "T4", "Under what combined provisions can a Provincial Assembly be dissolved, and who has that power?"),
    (18, "T4", "Trace the path of a Money Bill from introduction to assent — cite every Article involved."),
    (19, "T4", "What rights of an arrested person are guaranteed across Articles 9, 10, and 10A taken together?"),
    (20, "T4", "If Fundamental Rights conflict with a Principle of Policy, what does the Constitution say about precedence? Cite the controlling Articles."),
]

# Stratified split — each tier contributes 2-3 to each bucket
# val (10):     1, 3, 6, 8, 10, 11, 14, 16, 18, 20
# held-out (10): 2, 4, 5, 7, 9, 12, 13, 15, 17, 19
VAL_IDS = {1, 3, 6, 8, 10, 11, 14, 16, 18, 20}
HELDOUT_IDS = {2, 4, 5, 7, 9, 12, 13, 15, 17, 19}

assert VAL_IDS.isdisjoint(HELDOUT_IDS)
assert VAL_IDS | HELDOUT_IDS == set(range(1, 21))

val, held = [], []
for idx, tier, text in PROMPTS:
    record = {"id": idx, "tier": tier, "prompt": text}
    (val if idx in VAL_IDS else held).append(record)


def write(records, path):
    with path.open("w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"[✓] {len(records)} prompts → {path}")


if __name__ == "__main__":
    write(val, OUT_DIR / "val_prompts.jsonl")
    write(held, OUT_DIR / "heldout_prompts.jsonl")
    print("\nTier balance check:")
    for bucket_name, bucket in [("val", val), ("heldout", held)]:
        tiers = {t: sum(1 for r in bucket if r['tier'] == t) for t in ("T1", "T2", "T3", "T4")}
        print(f"  {bucket_name:8s}: {tiers}")
