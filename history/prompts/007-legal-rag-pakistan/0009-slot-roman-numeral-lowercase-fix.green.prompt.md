---
id: 0009
title: Slot extractor Roman-numeral lowercase fix (Q15 "Article de")
stage: green
date: 2026-06-07
surface: agent
model: claude-opus-4-8
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: (conversation — Fix 1 via TDD)
labels: [intent-router, slots, regex, bugfix, tdd, fundamental-rights]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/intent_router/slots.rs
  - CHANGELOG_DEV.md
  - history/prompts/007-legal-rag-pakistan/0009-slot-roman-numeral-lowercase-fix.green.prompt.md
tests:
  - "cargo test -p echo-ai --lib intent_router::slots — 11/11 pass"
  - "cargo test -p echo-ai --lib intent_router — 20/20 pass"
---

## Prompt

> yes, start with Fix 1 using TDD

(Context: the diagnosis of Q12/Q15 failures on the fundamental_rights 0.5B V2 eval
identified Fix 1 as the `section_path_re` Roman-numeral bug in
`intent_router/slots.rs`. User wants 100% on Qwen 0.5B; this is the first, highest-
confidence code fix toward that.)

## Response snapshot

Followed RED → GREEN. RED: added `head_noun_followed_by_word_is_not_a_section_path`
proving `extract("…articles dealing with…")` produced `["Article de"]`; watched it
fail with `got ["Article de"]` while the 8 prior tests passed. Diagnosed the cause:
the number-token class `[0-9IVXLCDM]+` ran under a global `(?i)`, so the Roman-numeral
letters i/v/x/l/c/d/m matched lowercase letters inside ordinary words ("dealing" →
"de"). Found the identical latent bug in the sibling-list scanner
`additional_numbers_re`.

GREEN: dropped global `(?i)` on both regexes; made only the head noun / `and`
connector case-insensitive via `(?i:…)` scoped groups, and the number token
case-sensitive — Arabic numbers (optional single-letter suffix) or UPPERCASE Roman
only. Preserved capture-group indices (head = group 1, number = group 2) so
`extract()`'s `cap.get(1)`/`cap.get(2)` are unchanged. Added 2 more tests
(sibling-scanner-trailing-word, uppercase-roman-still-parses). All green, no
regressions.

## Outcome

- ✅ Impact: Q15 (and any "article(s)/section(s) <word starting i/v/x/l/c/d/m>"
  query) no longer emits a phantom provision into the system prompt. Clears one of
  the two slot-extraction defects poisoning Q15's prompt.
- 🧪 Tests: `intent_router` 20/20 green; `slots` 11/11 (3 new). Watched RED fail
  for the expected reason before fixing.
- 📁 Files: `slots.rs` (2 regex edits + 3 tests), `CHANGELOG_DEV.md` entry.
- 🔁 Next prompts: Fix 2 (instrument over-capture, `slots.rs:35` grabbing leading
  verb "Identify") → Fix 3 (`query_router.rs:235` follow-up detector firing on
  intra-sentence pronouns, Q12) → re-run 15-prompt eval to confirm Q12/Q15 flip.
- 🧠 Reflection: the memory's "router fallback scrambles numbers" framing was
  imprecise; reading the traces showed retrieval was perfect and the cause was a
  garbage prompt. Diagnose-before-code paid off.

## Evaluation notes (flywheel)

- Failure modes observed: case-insensitive flag bleeding into a Roman-numeral
  character class — a general regex hazard whenever `(?i)` is global and a class
  encodes letters that double as ordinary text.
- Graders run and results (PASS/FAIL): unit tests PASS (20/20 intent_router).
  Full 15-prompt eval re-run still pending (after Fix 2/3).
- Prompt variant (if applicable): n/a (code fix).
- Next experiment (smallest change to try): Fix 2 — constrain `instrument_re` so a
  sentence-leading imperative verb (Identify/List/Explain/Describe) cannot start the
  `named` instrument group.
</content>
