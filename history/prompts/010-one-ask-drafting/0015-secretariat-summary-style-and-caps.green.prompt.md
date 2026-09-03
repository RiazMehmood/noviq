---
id: 0015
title: Secretariat-summary refactor — style/structure + citation caps & single-section dedup
stage: green
date: 2026-06-16
surface: agent
model: claude-opus-4-8
feature: 010-one-ask-drafting
branch: 011-postgres-pgvector-port
user: Riaz Mehmood
command: (implement — spec/tasks-secretariat-summary-refactor)
labels: [drafting, one-ask, secretariat-style, citation-caps, dedup]
links:
  spec: specs/010-one-ask-drafting/spec-secretariat-summary-refactor.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/drafting/one_ask/compose.rs
  - src-tauri/src/services/drafting/one_ask/skeleton.rs
  - src-tauri/src/services/drafting/one_ask/research.rs
  - src-tauri/src/services/drafting/one_ask/verify.rs
  - src-tauri/tests/one_ask_acceptance.rs
  - CHANGELOG_DEV.md
tests:
  - cargo test --lib drafting::one_ask (57/57 green; +8 new)
---

## Prompt

Higher-ups' 11-change brief to turn one-ask drafts into Government of Sindh secretariat summaries.
After discussion the architect locked: D1 verbatim-compressed citations (not model-paraphrased),
D2 generic domain logic (no keyword lists), D3 one skeleton + type profiles, D4 structural now /
precision after re-index. User: "go with ur recommendations" then "go ahead" for implementation.

## Response snapshot

Wrote SDD spec + tasks extension, then implemented two corpus-independent slices:

Slice 1 (style/structure): Secretariat style profile in build_section_prompt (bans advocacy/
greetings/"we request"); §2–§5 told not to restate background; prayer = one sentence, no citations;
structured financial fallback; citation snippet 30→20 words (still verbatim char-slice).

Slice 2 (caps + dedup): Section.max_citations (BG0/Legal2/Fin1/Prop0/Prayer0, sum ≤3); research
skips cap-0 sections and gives each eligible section ≤cap cards; single-section ownership
(MAX_SECTIONS_PER_CARD removed → no cross-section duplication); anti-starvation force-adopt removed;
verify enforces caps (0-cap/over-cap citations dropped silently, non-existent still flags); scrub
literal "Paragraph N:" labels.

Build ops: rust-analyzer's auto `cargo check --all-targets` (different CARGO_HOME) kept grabbing the
target-dir lock + invalidating cache → repeated full rebuilds; killing it restored 5.2s incremental
test phase.

## Outcome

- ✅ Impact: drafts gain secretariat register (no filler), one-line prayer, no background repetition,
  shorter quotes, ≤3 citations total, single-section ownership (no cross-section duplication). Two-lane
  verbatim guarantee intact (D1).
- 🧪 Tests: cargo test --lib drafting::one_ask 57/57 (+8 new); acceptance assertion updated to 1..=3
  (PG-backed, not run this batch).
- 📁 Files: compose.rs, skeleton.rs, research.rs, verify.rs, one_ask_acceptance.rs, CHANGELOG_DEV.md.
- 🔁 Next prompts: rebuild+relaunch (NO re-index) → live Canon-scanner draft → judge SC-002/003/004/006;
  then doc-type profiles (US1), tabular re-extraction (US8), relevance floor + doc-set narrowing (US6).
- 🧠 Reflection: prompt-only instructions (no-restate, prayer brevity) were partly ignored by the 7B in
  the prior live run — confirms the deterministic layer (caps/dedup) is what actually moves the needle.

## Evaluation notes (flywheel)

- Failure modes observed: 7B ignores soft prompt instructions; cap-0 sections must DROP (not flag) valid
  citations to keep prose clean while still flagging genuinely non-existent markers.
- Graders run and results (PASS/FAIL): one_ask unit suite PASS (57/57).
- Prompt variant: n/a
- Next experiment: live re-run; confirm ≤3 citations, zero cross-section repeats, citation-free prayer,
  Direct Selection routed into Legal.
