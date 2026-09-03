---
id: 129
title: Footnote tail-outlier rule kills false provision labels
stage: green
date: 2026-07-29
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform) "yes, start step 3"
labels: [ingestion, heading-detection, false-labels, citations, tests]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - path: src-tauri/src/services/heading_detector/regex_pass.rs
    change: added drop_footnote_tail_outliers + 4 constants; wired as the LAST pass in scan_with_profile_unit; 8 tests
  - path: CHANGELOG_DEV.md
    change: dated entry
tests:
  - name: cargo test --lib (VM, NOVIQ_TEST_PG_INSTALL_DIR set)
    result: PASS — 815 passed / 0 failed / 6 ignored
  - name: heading_detector::regex_pass filtered
    result: PASS — 43 passed / 0 failed
---

## Prompt

yes, start step 3

## Response snapshot

Fixed the last true parser defect from the corpus audit: footnote numbers minted
as provision labels.

**Diagnosis first, from real data.** Pulled the distinct provision-number set of
all 19 prod documents out of Postgres before writing any code. That made the
shape obvious: `Article 343` (Constitution, after a dense 1–280), `Section 1752`
(Civil Servants Act, after 1–26), `Section 156` (Dev Manual, after 1–23, 30) are
all **trailing outliers**, while every legitimate document in the corpus has a
top-of-run gap of exactly **1**.

**Why each existing guard misses them** — three different reasons, none fixable
by widening a threshold:

- `looks_like_year` rejects 1800–2099, so 1752 is *below* the floor.
- `drop_out_of_sequence_articles` is a streaming rule with a running max; its
  far-forward test needs `max_num >= 20 && n > 2*max && n-max > 300`. Article 343
  sits where the run is already ~230 (neither 2× nor +300); Section 1752 sits on
  p.8 where the run has only reached ~9 (fails the `>= 20` precondition);
  Section 156 is +133 (under the 300 cut).

**Fix**: `drop_footnote_tail_outliers`, a second **global** pass. Real provisions
form one dense run; a footnote number is an isolated island above its top, which
is only visible with the whole document in hand. Peels from the top while the gap
beats both an absolute floor (25) and a fifth of the span below.

Two design choices worth recording:

1. **It runs LAST**, after the dotted-leader and roster filters — it reasons
   about the shape of the whole run, so it must see the final set. A stray
   contents line surviving `strip_front_matter` would otherwise distort the very
   statistics it judges by. (I first wired it right after the streaming rule and
   moved it on that reasoning.)
2. **Fail-safes**, following the poison-guard precedent already in the file: off
   under 20 distinct numbers, top-of-run only, one number at a time, removals
   capped at 10%.

Tests: 8 added, fixtures taken from the real corpus number sets — one per false
label, a legitimate-corpus sweep, SPPRA's genuine internal holes (56, 59-60, 69,
86) which must be ignored, the Manual's `Section 30` (gap 7) which is run jitter
and must survive, both fail-safes, structural-heading pass-through, and an
end-to-end `scan_with_profile` case at the Constitution's exact shape where the
far-forward rule provably cannot help.

Ran on the VM (no local Rust toolchain) against `/opt/noviq/.cargo-target-test`
with `NOVIQ_TEST_PG_INSTALL_DIR=/home/appadmin/test-pg-install`. Without that env
var 69 DB-backed tests panic at `db/mod.rs:107` — the known false "69 failing"
baseline, not a regression. The prod checkout was used as the build tree and
restored to pristine afterwards (md5 verified); `deploy.sh` early-exits while
HEAD matches origin, and `cargo test` builds the debug profile, so the live
release binary was never touched.

## Outcome

- ✅ Impact: the three false labels are removed at index time. Gap reports stop
  lying (the Constitution's "62 missing" was entirely an artifact of 343) and
  ~14 chunks of Article-232 text stop being cited as a non-existent Article 343.
- 🧪 Tests: **815 passed / 0 failed / 6 ignored** (was 806 + 8 new + 1). The
  regex_pass module alone: 43 passed / 0 failed.
- 📁 Files: `regex_pass.rs` (+1 fn, 4 consts, 8 tests), `CHANGELOG_DEV.md`.
- 🔁 Next prompts: re-index Constitution, Civil Servants Act and Dev Manual so
  the fix takes effect on prod; then the genuine small gaps (SPPRA Rules
  56/59/60/69/86, Secretariat 90/131/153, APT 12, E&D 10, RoB 13); then the
  `graph_nodes` delete-cascade bug.
- 🧠 Reflection: the corpus itself was the best test fixture available. Pulling
  the real number sets first turned a threshold-tuning guess into a rule I could
  check against all 19 documents before writing it.

## Evaluation notes (flywheel)

- Failure modes observed: three guards existed for this family of defect and all
  three missed, each for a different structural reason — a sign the abstraction
  (streaming max) was wrong, not the constants.
- Graders run and results (PASS/FAIL): PASS — 815/815 lib, rule fires on exactly
  3 of 19 documents.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): after the re-index, re-run
  `corpus_gap_check.sql` and confirm the top three rows of section 1 collapse
  from 1725/131/62 "missing" to near zero.
