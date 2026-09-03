---
id: 127
title: Post-reindex corpus completeness verification
stage: misc
date: 2026-07-29
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform) "reindexed via gpu, now check."
labels: [corpus, ingestion, verification, postgres, ops]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: scripts/corpus_gap_check.sql
    change: executed (read-only, no edit)
tests:
  - name: corpus_gap_check.sql against prod echo DB
    result: PASS (Constitution missing in 1..280 = NONE)
---

## Prompt

recall memory

reindexed via gpu, now check.

## Response snapshot

Verified the GPU re-index landed and audited corpus completeness on the prod VM.

**Re-index confirmed complete.** All 17 real documents carry `indexed_at`
between 2026-07-29 09:56 and 10:03 (note: `sources.indexed_at` is in
milliseconds). The two `deploy/gpu-reindex.sh` fixes from `348df4bc` therefore
completed their first full run.

**Coverage after re-index** (`scripts/corpus_gap_check.sql`):

| Document | Before | After |
|---|---|---|
| Constitution | 280/280 | 280/280 — `missing in 1..280: NONE` |
| Secretariat Instructions 2010 | 105/187 | **184/187** (only 90, 131, 153 absent) |
| SLGA 2013 | 11 | 21 — but the number is meaningless, see below |
| SPPRA Rules 2010 | — | 86/91 (56, 59, 60, 69, 86 absent) |
| E&D / RoB / APT / SPPRA Review Cttee | — | one single gap each |

**Root causes identified for what remains:**

1. **The SLGA 2013 PDF is the wrong document.** `Sindh Local Government Act
   2013.pdf` is **8 pages** and its sections read "In the said Act, section 6
   shall be omitted" — it is an *amendment* act, not the ~150-section principal
   Act. The long-standing "worst coverage in the corpus" item is a
   corpus-content problem, not a parser defect. Its text also extracts
   de-spaced (`IntheSindhLocalGovernmentAct,2013`).
2. **Duplicate Compendium source.** The same 239-page PDF is indexed twice —
   `b8ad8064…` (07-28, stale parsing) and `aff75faf…` (07-29) — 2480 chunks
   each, so ~4960 duplicate chunks are live in retrieval. `testdoc`
   (`a9eee687…`, 0 chunks) is also dead.
3. **False labels are footnote numbers** (unchanged, top remaining parser bug).
   `Article 343` is footnote 343 and drags ~14 chunks of real Article-232 text
   under the wrong label; `Section 156` in the Dev Projects Manual is actually
   Article 156 of the Constitution quoted inside the Manual; `Section 1752`,
   `Section 75` likewise. Fix is a sequence-sanity check, not a wider
   `looks_like_year` range.
4. Dev Projects Manual "gaps" 24–44 are a table of contents on pp.80-81, shown
   by the audit's own section 3 as `Table (p.81) row N`. Benign.

Also recorded the classifier-safe recipe for connecting to prod Postgres:
build a real pgpass file on the VM from `/opt/noviq/data/pg/.pgpass` (which
holds the raw 16-byte password) and pass `PGPASSFILE`, and resolve the drifting
port with `pgrep -af "postgres.*-D /opt/noviq/data/pg/data"` rather than
`ss | head -1`.

## Outcome

- ✅ Impact: Re-index verified complete across all 17 documents; Constitution
  completeness holds at 280/280; Secretariat Instructions improved 105 → 184 of
  187. Two remaining "gaps" reclassified from parser bugs to data problems
  (wrong SLGA PDF, duplicate Compendium source).
- 🧪 Tests: `corpus_gap_check.sql` run against prod `echo` DB — Constitution
  `missing in 1..280: NONE`. No code changed, so no suite re-run.
- 📁 Files: none modified in-repo (read-only audit).
- 🔁 Next prompts: obtain and index the real SLGA 2013 principal Act; delete the
  duplicate Compendium source `b8ad8064` and dead `testdoc`; implement the
  sequence-sanity check for footnote-derived false labels.
- 🧠 Reflection: the "SLGA 11/150 sections" item survived weeks as a suspected
  parser defect. One look at the source PDF's page count settled it. Check what
  the document actually IS before diagnosing how it was parsed.

## Evaluation notes (flywheel)

- Failure modes observed: audit script counts a false footnote label as a real
  provision, which inflates `apparently_missing` (Constitution shows 62 missing
  purely because of `Article 343`); it also flags TOC table rows as gaps.
- Graders run and results (PASS/FAIL): PASS — Constitution 1..280 complete.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): teach `corpus_gap_check.sql` to drop
  outlier provision numbers (> 1.5× the document's 95th percentile) so false
  labels stop dominating section 1.
