---
id: 0006
title: Low-signal / probe guardrail (nudge not junk answer)
stage: green
date: 2026-07-14
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [guardrail, intent-routing, ux, fast-path]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/intent.rs
  - src-tauri/src/services/assistant/mod.rs
  - CHANGELOG_DEV.md
tests:
  - src-tauri/src/services/assistant/intent.rs::low_signal_matches_probes_not_real_questions
---

## Prompt

also add guardrail for greetings, single text like 'a', 'test' etc.

## Response snapshot

Greetings already had a fast-path (`is_greeting_or_intro` → `greeting` branch in `run_turn`). Added a
parallel guardrail for content-free probes so `a`, `test`, `123`, `...`, `asdf`, `qwerty` etc. no longer
fall through to full retrieval + a ~30 s (or ~130 s deep) inference that answers junk with junk citations.

- `intent::is_low_signal(query)` — true for empty/punctuation-only, ≤2-char alphanumeric skeleton, pure
  digits, or an exact match against a small probe/filler list. Conservative: a single meaningful word
  (`penalties`, `SPPRA`) or any real question (`test the SPPRA rules`) is NOT caught.
- New `nudge` branch in `run_turn`, right after the greeting branch (slash / open-assist / prebaked-summary /
  greeting still win first; open Notice-Assist session still captures short replies). Returns `NOVIQ_NUDGE`
  (example questions + `@`-scope hint), no retrieval, no citations, traced as `Low-signal / probe`.
- Sits before capability dispatch, so it also short-circuits the F025 agentic deep loop for probes.

## Outcome

- ✅ Impact: probes get an instant nudge instead of a wasteful inference + misleading citations; deep mode no longer runs a full agentic loop on `test`.
- 🧪 Tests: added `low_signal_matches_probes_not_real_questions` (both directions). VM `cargo test --lib --features llm,server intent` pending.
- 📁 Files: intent.rs (predicate + test), mod.rs (branch + NOVIQ_NUDGE), CHANGELOG_DEV.md.
- 🔁 Next prompts: build+deploy; extend probe list from query_log if new filler shows up.
- 🧠 Reflection: mirrored the existing greeting fast-path shape exactly to stay low-risk; ordering keeps assist/slash flows intact.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (not built/deployed).
- Graders run and results (PASS/FAIL): pending VM compile + unit run.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): watch query_log for probes that slip through; tune probe list / thresholds.
