---
id: 0025
title: Fix ingest array-valued fields parse failure
stage: green
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 009-lgd-drafting
branch: 009-lgd-drafting
user: Riaz Mehmood
command: none (manual-test failure report)
labels: [drafting, ingest, parser, defect, tdd]
links:
  spec: specs/009-lgd-drafting/spec.md
  ticket: null
  adr: history/adr/0012-model-assisted-template-ingestion-with-deterministic-verbatim-verify-gate.md
  pr: null
files:
  - src-tauri/src/services/drafting/ingest.rs
  - src-tauri/src/commands/drafting.rs
  - CHANGELOG_DEV.md
tests:
  - services::drafting::ingest::tests::parse_tolerates_array_valued_string_fields (new, RED→GREEN)
  - cargo test --lib drafting (48/48)
  - tests/lgd_ingest_gate.rs (2/2)
  - tests/lgd_drafting_acceptance.rs (1/1)
---

## Prompt

Template ingestion could not produce a usable segmentation after 3 attempts (model output is not a parseable slot array: invalid type: sequence, expected a string at line 6 column 64). Retry, or re-export the document as text-based PDF/TXT and try again.

(Reported while manually testing Phase D real-gold template ingestion through the running app; followed by "retry" after the RED test run.)

## Response snapshot

Systematic debugging (root cause before fix). Evidence: the preserved spike artifact
`~/Desktop/Noviq Documents reports/option_c_ingestion_spike_20260610/raw/mistral7b_freetext.output`
line 1273 shows Mistral-7B emitting list-like regions as JSON arrays of strings
(`"provision": ["C.C to:-", "1. The Section Officer…", …]`). `ModelProposedSlot` typed
`label`/`provision`/`text` as `Option<String>`, so strict serde failed with exactly the reported
error on every attempt; the tolerant chain (control chars, trailing commas) never handled type
shape. The spike never exercised the parser (output read by eye), so the gap shipped.

Fix (TDD): failing test `parse_tolerates_array_valued_string_fields` with the verbatim spike
shape → `string_or_lines` untagged string-or-Vec<String> deserializer on the three fields,
arrays joined with `\n` so the verbatim-verify gate still sees exact source lines. Second small
change: on parse failure, `ingest_template` now dumps the raw model output to
`/tmp/echo-ai-debug/ingest_attempt<N>.txt` (F006 pattern) so future live failures are
diagnosable. CHANGELOG_DEV entry added.

## Outcome

- ✅ Impact: live real-gold template ingestion unblocked; the most common 7B output shape (string arrays for C.C./multi-line regions) now parses; raw output preserved on failure.
- 🧪 Tests: lib drafting 48/48, lgd_ingest_gate 2/2, acceptance 1/1, full lib build clean.
- 📁 Files: ingest.rs (deserializer + test), commands/drafting.rs (failure dump), CHANGELOG_DEV.md.
- 🔁 Next prompts: user retries the ~18 min ingest; inspect proposed schema (boilerplate vs verified provisions) per FR-010a; then Phase E gap-tier observation.
- 🧠 Reflection: spike outputs must be run through the production parser, not read by eye — a one-line harness would have caught this before live test.

## Evaluation notes (flywheel)

- Failure modes observed: model emits arrays where schema expects strings; tolerant parser only covered lexical (not type) deviations.
- Graders run and results (PASS/FAIL): cargo suites PASS (48/48, 2/2, 1/1).
- Prompt variant (if applicable): SEGMENTATION_TASK unchanged.
- Next experiment (smallest change to try): if next live ingest still fails, read /tmp/echo-ai-debug/ingest_attempt*.txt for the new shape before changing anything.
