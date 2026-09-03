---
id: 158
title: Corpus run complete, GPU released, uncategorised residue and duplicates analysed
stage: general
date: 2026-08-17
surface: agent
model: claude-opus-5
feature: general
branch: deploy-idx
user: Riaz Mehmood
command: (conversational)
labels: [ops, corpus, gpu, categorisation, duplicates, sindh]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - scratchpad/classify_uncat.py (dry-run classifier, VM ~/classify_uncat.py)
  - scratchpad/dupes.sql (VM ~/dupes.sql)
  - VM ~/q.sh (psql helper: resolves drifting port from postmaster.pid)
  - VM ~/uncat_proposal.tsv (270-row proposal — NOT applied)
tests:
  - vLLM behaviour check: /v1/completions returned a real completion (PASS)
  - classifier spot-check: 28-row random sample, ~10-12 clearly wrong (FAIL — not applied)
---

## Prompt

check indexing progress?

(then, mid-turn) why 384 uncategorized? categories them also, and check for duplications also.

## Response snapshot

**Corpus run finished** 2026-08-16 13:44 PKT — 39/39 departments, log ends
`corpus run COMPLETE`, tmux session gone. DB: **4,074 indexed, 4 failed, 0 pending,
304,617 chunks, 50 departments**. All 4 failures are old test uploads
(`source scanned pdf` ×2, a ChatGPT image, `test img`) — no corpus document failed.

**GPU released** (architect approved). `sudo bash /opt/noviq/gpu-release.sh` →
`noviq-vllm`, `noviq-server`, `noviq-gpu-index.timer` all active; GPU back to
10,873 MiB. Verified **by behaviour**, not logs: `/v1/completions` on
`Qwen/Qwen3-8B-AWQ` returned a real generation. Chat had been down ~31 h.

**The 384 uncategorised split cleanly**: 270 in a literal `_uncategorised/` corpus
folder (the title-classifier residue — 3,961 − 3,691 = 270 exactly) and 114
non-corpus `uploads/` (test/user files, uncategorised by design).

**Categorisation attempted and REJECTED on quality.** Three passes with
Qwen3-8B-AWQ against the 50-department list:
1. free-text department name → 132/289 resolved, 30 malformed (name matching too fragile)
2. numeric answer → 133/270 resolved, 137 abstain, 0 malformed
3. numeric answer + first 1400 chars of document text → **228/270 (84%) resolved, 42 abstain**

Yield looked good, but a 28-row spot-check showed ~10–12 clearly wrong
(`INDUS VESSELS ACT 1863` → Universities & Boards; `SINDH CENSORSHIP OF FILM RULES`
→ Sindh Revenue Board; `EYE SURGERY RESTRICTION ORDINANCE` → Labour;
`DARUL ATFAL SUPERVISION AND CONTROL ACT` → Revenue Board, though its duplicate pair
proves Social Welfare). Two near-identical `SINDH EMPLOYEES SPECIAL ALLOWANCE`
ordinances got different departments. The Revenue Board (54) + Excise (40) skew —
41% of all assignments — is the model defaulting, not signal. ≈60% accuracy, below
the existing keyword classifier's 83.6% holdout. **Nothing written to the DB**;
proposal parked at `~/uncat_proposal.tsv`.

Root cause of why this residue is hard: **the source filenames are themselves
corrupt** — `IDEO_FILM_DEALIER`, `INDH_HINDU_HEIRS`, `CIVIL_DEFNECE`,
`BADIN_DEVELOPENT` are exactly what the harvester wrote. The residue is enriched in
garbage titles, which is why the keyword classifier dropped them in the first place.

**Duplicates (exact `file_hash`)**: corpus 53 groups / 53 redundant copies —
37 within one department (harvest wrote the same PDF under two IDs, e.g.
`X.pdf` + `X__S3076.pdf`) and 16 across departments (same instrument published by
two departments — the known rtw "department = publisher" issue). Redundant corpus
sources carry **7,672 chunks**. Uploads: 10 groups / 84 redundant copies.

## Outcome

- ✅ Impact: chat restored after ~31 h outage; corpus load confirmed complete and clean; uncategorised residue and duplicate load quantified.
- 🧪 Tests: vLLM verified by real generation (PASS). Classifier spot-check FAILED → change not applied.
- 📁 Files: no repo code changed; analysis scripts in scratchpad + VM home.
- 🔁 Next prompts: decide categorisation route for the 270 (bigger model / two-stage shortlist / accept abstain / manual review); decide whether to dedup the 53 corpus copies.
- 🧠 Reflection: high resolve-rate is not accuracy. The numeric-answer fix removed *parse* failures and made the run look far better (133 → 228) while the underlying judgement stayed poor — measuring yield would have shipped ~90 wrong department labels into production.

## Evaluation notes (flywheel)

- Failure modes observed: (a) free-text label matching against a 50-item list is fragile — model restates the title first; (b) `-c` swallows a following `-F` in psql argv; (c) PG port drifts on `noviq-server` restart — must be read from `postmaster.pid` every time; (d) LLM defaults to large/salient categories when uncertain rather than abstaining.
- Graders run and results (PASS/FAIL): manual 28-row spot-check — FAIL (~60% accuracy vs 83.6% incumbent).
- Prompt variant (if applicable): v3 (numeric + document text) is the best of three but still not good enough to apply.
- Next experiment (smallest change to try): two-stage — cheap keyword/embedding shortlist of 5 candidate departments, then ask the model to pick among 5 instead of 50; re-score on the same 28-row sample before any write.
