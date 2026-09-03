---
id: 0004
title: Wave B US4 deterministic drafting green
stage: green
date: 2026-06-18
surface: agent
model: claude-opus-4-8
feature: 012-doc-intelligence-production
branch: 012-doc-intelligence-production
user: Riaz Mehmood
command: /sp.implement (Wave B / US4)
labels: [green, drafting, deterministic, us4, citation-grade]
links:
  spec: specs/012-doc-intelligence-production/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/drafting/one_ask/compose.rs
  - src-tauri/src/services/drafting/one_ask/mod.rs
  - src-tauri/src/commands/one_ask.rs
  - src-tauri/tests/one_ask_acceptance.rs
  - CHANGELOG_DEV.md
  - specs/012-doc-intelligence-production/tasks.md
tests:
  - "cargo test --lib = 476 passed, 0 failed (+6 deterministic compose tests)"
  - "cargo test --test one_ask_acceptance = 4 passed (incl. PanicWriter deterministic test)"
---

## Prompt

commit and proceed to us4

## Response snapshot

Committed Wave A (d6043037), then implemented US4 — deterministic drafting.

- `compose::compose_deterministic` emits each of the 7 skeleton moves from fixed
  secretariat templates + verbatim slots; [CITE:n]/[FIG:n] markers placed
  deterministically, per-section cap honored. The model authors no prose.
- `run_pipeline` gained `deterministic: bool`; production passes true
  (commands/one_ask.rs); legacy model lane retained as fallback (false) and used
  by the existing verify-machinery acceptance tests.
- Verification reused unchanged: verify::assemble_and_verify already does the
  decidable verbatim-splice + cap + flag gate. Deterministic prose carries no
  numbers/quotes, so G1 holds by construction — no separate whitelist needed.
- Tests: 6 compose unit tests + acceptance deterministic_draft_never_calls_writer
  _and_verifies using a PanicWriter (fails if the LLM is called). All green.

No re-index (compose-time). The ¶2 fabrication surface is structurally gone.

## Outcome

- ✅ Impact: drafting can no longer fabricate prose; every number/quote/figure
  arrives only via a marker resolved verbatim. cargo test --lib 476/0; acceptance 4/4.
- 🧪 Tests: 476 lib + 4 acceptance (incl. PanicWriter proof).
- 📁 Files: compose.rs, mod.rs, commands/one_ask.rs, one_ask_acceptance.rs, CHANGELOG, tasks.md.
- 🔁 Next prompts: optional US3 (heading split-action/confidence) + US5 (table engine,
  needs re-index); live re-run of the Canon-scanner ask to eyeball tone.
- 🧠 Reflection: the cheapest correct fix was to reuse the existing verbatim-splice
  gate and only swap the prose source from LLM → templates — no new verifier needed.

## Evaluation notes (flywheel)

- Failure modes observed: none; PanicWriter confirms zero LLM invocation in the deterministic lane.
- Graders run and results (PASS/FAIL): cargo test PASS (476 lib / 4 acceptance).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): live Canon-scanner re-run; if templates read stiff,
  add the spec's optional sentence-level connector whitelist.
