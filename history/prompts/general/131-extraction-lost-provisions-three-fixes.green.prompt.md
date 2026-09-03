---
id: 131
title: Extraction lost 13 provisions while scoring itself perfect — three fixes
stage: green
date: 2026-07-30
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "fix all three"
labels: [extraction, pdfplumber, ingestion-sidecar, text-normalizer, quality-gate, compendium]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/text_normalizer.rs
  - src-tauri/src/services/parsers/pdf.rs
  - src-tauri/sidecars/ingestion-sidecar/ingestion_sidecar.py
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib (VM, throwaway tree, --no-default-features --features server)"
  - "baseline control 777 passed / 70 failed; with changes 783 passed / 69 failed; zero regressions"
  - "new: text_normalizer::tests::restores_the_space_before_an_opening_sub_clause"
  - "new: text_normalizer::tests::does_not_split_a_year_from_a_parenthesis"
  - "new: pdf::completeness_tests::counts_every_provision_ordinal_shape"
  - "new: pdf::completeness_tests::ignores_non_anchors"
  - "new: pdf::completeness_tests::tolerates_jitter_but_refuses_real_loss"
  - "sidecar verified directly on Compendium / Constitution / Delegation / Secretariat PDFs"
---

## Prompt

fix all three

(Context: the three defects diagnosed earlier in the session — the pipeline was
losing 13 provisions from the SLGA Compendium while reporting `quality_score: 1`.)

## Response snapshot

Three fixes, smallest viable diff each, verified before claiming.

1. `text_normalizer::provision_dot_space` — allow an opening paren after the
   ordinal (`19.(1)`), not only a capital or quoted term. Year guard untouched
   (the 1–3 digit ordinal is what blocks `1996.(a)`). Recovers 8 provisions.
2. `ingestion_sidecar` — the catchline crop's safety check summed anchors across
   the sample, letting re-flow artefacts on body pages pay for the table of
   contents losing all 13. Rebuilt around the anchor-loss RATE (Constitution 0%,
   Compendium 7%, Delegation 70%), which is what actually distinguishes a
   catchline margin from a hanging indent. Recovers 5 provisions AND keeps the
   53 catchline titles.
3. `pdf::tier0_is_complete` — cross-check Tier 0's provision-anchor count against
   `pdftotext` before accepting it, because `text_quality::score` grades one tier
   alone on shape and truncated-but-fluent text passes every test it applies.
   Fails open.

## Outcome

- ✅ Impact: Compendium goes from 13 lost provisions to 0 (5 by the crop fix, 8 by
  the normalizer fix), with all 53 catchline titles retained and the body still
  cropped on 28/30 pages. Constitution behaviour identical (313 titles, 0 pages
  kept whole, no added cost). Delegation/Secretariat unchanged. Takes effect on
  RE-INDEX only.
- 🧪 Tests: 5 new; matched control run on the VM shows zero regressions.
- 📁 Files: 3 source + changelog.
- 🔁 Next prompts: Compendium segmentation (13 instruments in one file); commit
  and deploy decision; re-index.
- 🧠 Reflection: the bake-off I was running for an unrelated reason caught a
  regression in my own fix. Without it I would have shipped a change that
  recovered 5 provisions by destroying 53 provision titles.

## Evaluation notes (flywheel)

- Failure modes observed: (a) my first fix traded one defect for another;
  (b) my second attempt produced garbage titles on hanging-indent documents.
  Both were caught by comparing against real corpus PDFs, not by reasoning.
- Graders run and results (PASS/FAIL): PASS — provisions lost 13 → 8 → 8 with
  titles 53 → 0 → 53 across the three iterations; Constitution regression PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): split the Compendium on its own TOC
  page ranges and re-run the gap check per constituent instrument.
