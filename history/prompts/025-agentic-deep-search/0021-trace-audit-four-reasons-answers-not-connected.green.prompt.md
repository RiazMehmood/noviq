---
id: 0021
title: Trace audit — four reasons the answers were not connected
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [conversation-memory, intent-routing, trace, audit, real-traffic-gold]
links:
  spec: specs/015-bureaucrat-chat/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/intent_router/classifier.rs
  - CHANGELOG_DEV.md
tests:
  - "vm: query_router:: — 71/71 PASS (incl. PG-backed, with NOVIQ_TEST_PG_INSTALL_DIR)"
  - "vm: intent_router:: — 47/47 PASS"
  - "query_router.rs::real_chat_2026_07_16_every_followup_keeps_its_history (real-traffic gold)"
  - "query_router.rs::punctuation_does_not_switch_memory_off"
  - "query_router.rs::known_limit_capitalisation_suppresses_anaphora (documents a limit)"
  - "query_router.rs::prior_turns_block_uses_every_pair_the_caller_hands_it"
  - "classifier.rs::deadline_and_record_words_are_not_amendment_timelines"
---

## Prompt

still it is not fired after 8 questions. and answers are also not connected with each other, check the
traces and first audit them at granuall level from each percpective from all edges, then first plan
completely then decide what do to, then first discuss with me in simple language then after my call
implement that.

## Response snapshot

Audited the eight REAL traces (`$ECHO_LOG_DIR/queries` = `/opt/noviq/echoAI/logs/ab_rerank/queries`
in prod — ⚠️ `echoAI/logs/queries` is stale from June and misled me first). **Five of eight turns
broken, four independent causes.** Planned, discussed in plain language, architect approved, then
implemented.

**RC1 — a trailing `?` switched memory off.** `is_followup_query` padded RAW text and matched
`" that "`; `"…for that?"` yields `" that?"` → no match → `prior_turns_block` empty → **zero history
to the model**. Word-boundary matching on non-word-boundaries. Killed Q3. Q7/Q8 died on vocabulary
(`"first question"` but not `"first one"`; `"i asked"` but not `"i was asking"`; `"this chat"` but not
`"my chat"`). Fix: normalise punctuation → spaces before matching (keep `-`, `/`); broaden the
conversation-reference list.

**RC2 — bare "timeline"/"history" skipped the corpus.** `classifier.rs` weight-1.0 cue →
`Intent::Timeline` → `Capability::AmendmentChain` = SQL/graph, **no retrieval, no model**. Q4 "what
about the appeal timeline?" returned 0 links in **7 ms** having read no law. "timeline" = deadline
here, "history" = record. Anchored the words to amendment/version language.

**RC3 — the trace lied, which is why this hid.** `Prior turns` counted AVAILABLE pairs, printing
"2 pairs from history" on the same turns that printed `history 0`. Two contradicting numbers in one
trace, unread for weeks. Third instance of the same disease this week. Now derived from the injected
block + names the gate: `⚠ DROPPED — is_followup_query judged this query self-contained`.

**RC4 — window 2 → 4 exchanges** (re-land `325618ab`): `prior_turns_block` re-truncated on top of the
caller's `recent_turns_verbatim` trim, so Q6 "what was my first question?" could only see back to Q4.
Safe now that `law_overhead` measures the block (`9f40d4aa`).

**The banner was right to stay quiet.** Measured: overhead 1520–1735 tok = **34–38%** of the 4512
limit; 80% needs ~3610. The chat *structurally cannot* fill up while history is capped. The premise
was inverted — not "too full" but "remembers almost nothing". Ceiling after RC4 ≈ 51%. Left dormant;
explicitly did NOT lower the threshold to manufacture an alert.

## Outcome

- ✅ Impact: 3 of 5 broken turns fixed outright (RC1), 1 more (RC2) now reads the law instead of
  returning an empty SQL lookup in 7 ms, memory doubled (RC4), and the trace can no longer hide it
  (RC3).
- 🧪 Tests: `query_router::` 71/71, `intent_router::` 47/47. The architect's 8 real questions are now
  a permanent gold test.
- 📁 Files: query_router.rs, classifier.rs, CHANGELOG_DEV.md.
- 🔁 Next: replay the same 8 questions and read the traces (`history > 0` on 3/7/8; Q4 → Guidance with
  real retrieval; every `Prior turns` line agreeing with its `history N`). Then: ordinal recall via
  `seq` lookup (graph), carry-over, deep cost-guard.
- 🧠 Reflection: **two tests stopped me shipping something wrong in one session.** (1) My
  capitalisation "fix" broke the Q12 regression — the trailing-pronoun heuristic cannot distinguish
  `"…for that?"` (no antecedent) from `"…act on them?"` (antecedent in-sentence); reverted and
  documented as a known limit. (2) `measured_overhead_grows_…` asserted `< 700` tok, silently encoding
  the old 2-pair window; it correctly failed on RC4 and needed rewriting to assert the bound that is
  really this function's job. **The audit should have come before the banner shipped.**

## Evaluation notes (flywheel)

- Failure modes observed: I shipped a banner (`22e8335a`) that could not fire, and only proved it
  after the architect tested. Root cause of MY error: reasoned about the budget from the code that
  computes it, never from real traffic. The trace audit took ~20 minutes and answered everything.
- Also: forgot `NOVIQ_TEST_PG_INSTALL_DIR` → 7 phantom "failures" (the known libssl red herring,
  already in memory). Read the gotcha before running the tests, not after.
- Graders run and results (PASS/FAIL): query_router 71/71 PASS; intent_router 47/47 PASS.
- Next experiment (smallest change to try): re-run the 8-question chat; diff the new traces against
  the audited ones turn-for-turn.
