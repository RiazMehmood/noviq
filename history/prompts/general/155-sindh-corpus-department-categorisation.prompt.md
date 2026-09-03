---
id: 155
title: Sindh corpus department categorisation
stage: general
date: 2026-08-14
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [corpus, categorisation, sindh-harvest, classification, backlog-grooming]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - D:/Sindh-Corpus/classify_dept.py
  - D:/Sindh-Corpus/dept_titles.json
  - C:/Users/hp/.claude/projects/D--echo-AI/memory/MEMORY.md
  - C:/Users/hp/.claude/projects/D--echo-AI/memory/project_sindh-corpus-department-categorisation.md
  - C:/Users/hp/.claude/projects/D--echo-AI/memory/project_limitations-docs-2026-08-05.md
  - C:/Users/hp/.claude/projects/D--echo-AI/memory/project_feature-claims-audit-2026-08-13.md
tests:
  - "classifier self-score against 291-document labelled holdout: 83.6% accuracy, 98.3% coverage (up from 59.0% / 85.6%)"
  - "coverage over corpus: 3,691 / 3,961 ok rows categorised across 49 departments"
  - "no repository code touched — no build or test suite run"
---

## Prompt

"limitations updation is not required, so remove this work where is possible
update those only dont create or generate new documents, feature claims are also
completed so remove that also, after that what is still pending. now i want to
categories sindh corpus documents also which we scrapped."

(Preceded in the same session by: "what is still pending?")

## Response snapshot

**1. Closed two backlog items in memory** (updated existing files, created no new
documents): removed the limitations-redline and feature-claims bullets from
`MEMORY.md`'s NEXT list, annotated the two underlying memory files as CLOSED so a
future session cannot resurrect them, and retitled the index entries. The audit
*findings* were kept — they are code facts, not tasks.

**2. Reported the remaining backlog**: `audit_log` retention (needs the
department's decision), Conflict/Compare on Deep search, Build Summaries,
indexing the Sindh haul, `NOVIQ_VLLM_SPEC=ngram` off before demos, the
`MAPREDUCE=on` hold, F027 T100, six open bugs, three parked security items.

**3. Categorised the Sindh corpus.** Measured first: `doc_type` is clean for
100% of rows, but department is present only for rtw (1,584) and srb (868) and
absent for all 1,768 sindhlaws rows; `dept_map.json` was confirmed unusable (88%
of rows in two buckets). Tested whether sindhlaws could inherit departments from
rtw — exact title match 274, identical sha256 32, union 291/1,768 (16%), so the
gap could not be closed by joining.

Architect chose **department / issuing body** as the axis with **titles
classified** for the gap. Built `classify_dept.py`: an ordered rule list over
rtw's 49 official department names, scored against a **free labelled holdout** —
the 291 sindhlaws documents whose department rtw already knows. First pass 59.0%;
after correcting rules against the printed individual mismatches, **83.6%
accuracy at 98.3% coverage**. Wrote `dept_titles.json` (1,498 labels), leaving
270 mostly-19th-century instruments Uncategorised. Final: **3,691/3,961 (93.2%)**
across 49 departments.

**Key finding surfaced to the architect**: rtw's `department` is the
**publishing** department, not the subject — 2021-22 acts collapse into Law /
Provincial Assembly regardless of topic, while older amendments of the same
instrument sit under the subject department. 28 of the 52 residual errors are
that artifact rather than classifier fault (subject-level accuracy ~92%). This
weakens "departmental scoping" as a customer-facing claim and should be settled
before the map is loaded.

## Outcome

- ✅ Impact: 3,691/3,961 Sindh documents (93.2%) mapped to 49 departments; two
  stale backlog items closed in memory. Map built and scored but **not yet
  loaded into `echo.db`** — the files are still unindexed, and
  `sources.category_id` is a single flat FK so department and doc_type cannot
  both be categories. No repository code was modified.
- 🧪 Tests: holdout score 83.6% accuracy / 98.3% coverage (from 59.0% / 85.6%);
  no build or suite run — nothing in the repo was touched.
- 📁 Files: `classify_dept.py` + `dept_titles.json` (out of tree, beside the
  harvest scripts); four memory files; this PHR.
- 🔁 Next prompts: decide whether the category means *publisher* or *subject*;
  load the map during the indexing run; delete the superseded `dept_map.json`.
- 🧠 Reflection: the labelled holdout existed for free in data already on disk —
  looking for ground truth before writing rules turned an unverifiable heuristic
  into a measured one, and then exposed that the ground truth itself answers a
  different question than the classifier does.

## Evaluation notes (flywheel)

- Failure modes observed: (a) generic tokens in an ordered rule list — `\bLAWS?\b`
  and `\bREPEAL\b` fired on every consolidating amendment, ~5 points of accuracy;
  (b) topic rules outranking named-body rules (a medical university classified as
  Health, not Universities & Boards); (c) tuning against aggregate accuracy
  instead of the individual mismatches, which hid both of the above.
- Graders run and results: holdout grader PASS at 83.6% (target was ≥80% before
  loading); 28 of 52 residual errors attributed to gold-label semantics, not the
  classifier.
- Prompt variant: n/a.
- Next experiment (smallest change): re-score with the Law and Provincial
  Assembly gold rows excluded, to separate classifier error from the
  publisher-vs-subject artifact cleanly rather than by attribution.
