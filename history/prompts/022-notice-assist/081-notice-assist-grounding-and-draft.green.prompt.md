---
id: 081
title: Notice Assist grounding and cited draft
stage: green
date: 2026-07-10
surface: agent
model: claude-opus-4-8
feature: 022-notice-assist
branch: 022-notice-assist
user: Riaz Mehmood
command: sp.implement
labels: [notice-assist, green, extraction, slot-loop, grounding, drafting]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/capabilities/notice_assist/extract.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/ask.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/compose.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/mod.rs
  - specs/022-notice-assist/tasks.md
  - CHANGELOG_DEV.md
tests:
  - "VM cargo check --release --features llm,server,cuda = EXIT 0"
  - "VM cargo test --lib (FULL) = 645 passed / 0 failed / 5 ignored"
  - "notice_assist targeted = 35 passed / 0 failed (17 notice_assist tests)"
---

## Prompt

continue work

(after /login on a fresh account; resuming Increment 2/3 of /sp.implement for 022-notice-assist)

## Response snapshot

Confirmed Increment 2 green on the VM (CHECK_EXIT=0, 28 passed), then built and verified
**Increment 3 (US4 + US5)**:
- `compose.rs` — `ground()` retrieves each invoked rule from the CORPUS (notice = premise,
  research R7) via `retrieve_for_mapreduce`, accepting a hit ONLY when the retrieved provision
  verbatim contains the normalized reference ("Rule-12 (2)" ≡ "rule 12(2)"). A miss ⇒ `found=false`
  ⇒ explicit ⚠ "not found in the indexed corpus" — never quoted from model memory (FR-008/011).
  `draft()` is deterministic assembly (NO LLM) of the show-cause / audit-para templates +
  documents-to-attach checklist; unanswered slots render as ⚠ gaps. `citations()`/`gate_context()`
  feed the citation gate. 7 unit tests.
- `mod.rs` — Ground/Draft phases return a real CITED TurnResponse (Capability::Draft); Done
  re-serves; "draft it" after Done re-grounds (fixed by promoting phase before the match rather
  than recursing).

One compile error caught and fixed on the VM: E0106 missing lifetime on `cited_for` (two input
lifetimes → ambiguous return borrow) → `fn cited_for<'f>(frame: &'f Frame, …) -> Option<(usize, &'f CitedRule)>`.

Final verification: cargo check EXIT 0; targeted tests 35/0; **FULL lib suite 645 passed / 0 failed**
(no regressions; the earlier classifier fix stays green). Frontend already green.

Honest deviation recorded: T002/T027 (analysis prompt + reply templates as TOML config fragments,
Principle VI) were implemented as CODE CONSTANTS — left open in tasks.md and flagged in CHANGELOG.

## Outcome

- ✅ Impact: F022 MVP (US1–US4) + US5 audit-para variant functionally complete and fully green.
- 🧪 Tests: 17 notice_assist unit tests; full suite 645/0.
- 📁 Files: compose.rs (new impl), mod.rs (phases), tasks.md, CHANGELOG_DEV.md.
- 🔁 Next prompts: T038 prompt-eval on real notices; T002/T027 move prompt+templates to TOML;
  T040 corpus-coverage GA gate; then merge to release.
- 🧠 Reflection: making `draft()` deterministic (no LLM) is what makes the integrity leash
  enforceable — the model can only ask questions and read the document; it can never author law.

## Evaluation notes (flywheel)

- Failure modes observed: E0106 lifetime (fixed). Extraction quality on REAL documents still
  unmeasured — fixtures only.
- Graders run and results (PASS/FAIL): VM cargo check PASS; targeted tests PASS (35/0); full suite
  PASS (645/0); npm build PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): T038 — run the extraction prompt over 3–5 real
  notices/audit paras; measure JSON-parse success + allegation recall; tune before GA.
