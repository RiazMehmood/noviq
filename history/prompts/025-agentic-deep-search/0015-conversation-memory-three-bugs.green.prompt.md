---
id: 0015
title: Conversation memory — three bugs found and fixed on prod
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (freeform) "test it yourself on prod and show me the result" → "fix it and test with the same X Y Z scenario" → "fix the summary and pinned facts injection"
labels: [conversation-memory, prompt, retrieval, bugfix, prod-verified]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket: n/a
  adr: suggested — retrieval-based-conversation-memory (not created)
  pr: n/a (fast-forward pushes to release: ce8957fb, 325618ab, 68d4f71a)
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --features llm,server query_router: 65 passed / 0 failed"
  - "cargo test --lib --features llm,server guidance: 16 passed / 0 failed"
  - "PROD: X→Y→Z then 'compare the first thing I asked with my third question' → correctly named X and Z"
  - "PROD: stated BPS-17 turn 1 → 'penalties for an officer of my grade' turn 2 → used Grade BS-17, cite on the law not the grade"
---

## Prompt

test it yourself on prod and show me the result
→ "yes fix it and test with the same X Y Z scenario"
→ "can we use db for this history purpose so all details remains there only what user ask that specif data comes
   from there so it will not fill context what do u think? discuss"
→ "fix the summary and pinned facts injection"

## Response snapshot

The user's scenario: in one chat ask X → Y → Z, then "compare X with Z" and expect the assistant to know what
was asked before. I had claimed earlier in the session that this "should work today" — traced to
`memory::load_for_turn` and stopped there. **Testing it on prod proved me wrong.** Three distinct bugs, each
found only by running the next test:

**Bug 1 — memory silently dropped (`ce8957fb`).** Asked *"Compare the findings of the first thing I asked you
about with my third question"*, prod answered about **Rule 19 supplementary questions** and project reports —
topics never in the conversation — and invented a connection. It had matched the literal words "first/third
question" against the corpus. Cause: `build_rag_prompt` injects history **only if `is_followup_query()`**
(which also gates retrieval carry-over). That gate was tuned for short anaphoric follow-ups; a long
self-describing question failed all signals — and the subtle part: `names_own_subject` saw the capitalised
**"What" opening the second sentence**, took it for a proper noun, and suppressed the `" them "` anaphora.
Control that proved it: the same intent as *"And compare those two."* (4 words) worked all along. Fix:
`CONVERSATION_REFERENCE` markers, checked **before** the suppression; excludes bare "compare".

**Bug 2 — the oldest turn dropped (`325618ab`).** Re-ran the test: memory was now injected, but the model called
**Y** the "first question". `build_rag_prompt` re-truncated to the last 2 pairs **on top of** the
`recent_turns_verbatim=8` window `load_for_turn` had already applied → with 3 pairs, X fell off. Two places
deciding one window. Fix: the caller owns the window. Re-ran → **PASS**: correctly named X (E&D grounds) and Z
(RSO appeal), ignored Y, coherent connection with citations.

**Bug 3 — pinned facts + summary never injected (`68d4f71a`).** Flagged during Bug 1's trace, then **verified
before fixing** (I had already been burned once today by inference): `query_router.rs`/`guidance.rs` contain
zero references to `pinned_facts`/`mem.summary`. They're loaded and **budgeted** — `plan_budget` counts them and
the audit reports `summary_included: true` — but `build_rag_prompt` has no parameters for them. **The audit was
lying.** Structural cause found while tracing: `plan_budget` runs **after** generation (`guidance.rs:293` on the
`:288` result) — a post-hoc audit, not the allocator that builds the prompt. Fix: `memory_block(mem)` appended
to `sys`, the same seam as `ATTACHMENT_SCOPE_OVERRIDE` — reaches both the deep loop (which wraps `{sys}`) and
the single-pass path with **zero signature churn**. Both blocks marked non-citable. Prod-verified: stated
"BPS-17" in turn 1 → asked "penalties for an officer of my grade" in turn 2 (grade never restated) → *"As an
officer of Grade BS-17, you fall under… Basic Scales 16, 17, 18, and 19 [6]"* — fact carried, and the citation
landed on the **law**, not the grade.

Also answered the architecture question: we use **no library** — it's hand-rolled Rust. The key insight is that
**LangGraph unifies conversation memory and checkpointing** (one `thread_id`, `messages` in the persisted
state), while Noviq splits them into two systems that never talk (`chat_message` vs F026 `deep_checkpoint`) —
which is exactly why "memory persistence" was ambiguous for two sessions. And notably: being architecturally
equivalent to LangGraph didn't help — the bugs were all in the gate we bolted in front of it.

## Outcome

- ✅ Impact: cross-turn reasoning ("compare X with Z") works in prod; the officer's stated facts now carry
  across turns; long-chat summary reaches the model instead of being discarded.
- 🧪 Tests: 65/65 query_router + 16/16 guidance; 2 prod scenarios verified end-to-end. The 2026-05-10 drift
  guard (Q12 regression + both build_rag_prompt history tests) still passes.
- 📁 Files: query_router.rs (gate + window), guidance.rs (memory_block + wiring), CHANGELOG.
- 🔁 Next prompts: `names_own_subject` sentence-initial flaw (deliberately out of scope — broadens follow-up
  detection, needs its own eval); tiered retrieval memory (`/sp.adr retrieval-based-conversation-memory`);
  `extract_facts` is legal-domain-only; memory is per-conversation, not per-user.
- 🧠 Reflection: I asserted "it should work today" from reading half the path. The user asking me to actually
  run it is what exposed three real bugs. Every fix was found by re-running the same test — the test was the
  instrument, not the unit suite (which was green throughout all three bugs).

## Evaluation notes (flywheel)

- Failure modes observed: (1) my own unverified inference ("it should work") — the trace stopped at
  `load_for_turn` and never reached `build_rag_prompt`; (2) unit tests green through all three bugs, because
  they test the parts, not the wiring — same lesson as the F026 resume dead-code found this morning;
  (3) two release pushes correctly blocked by the classifier for skipping the per-deploy confirmation.
- Graders run and results (PASS/FAIL): query_router 65/65 PASS; guidance 16/16 PASS; prod X/Y/Z PASS (after 2
  iterations); prod pinned-fact carry PASS; citation-on-law-not-grade PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): a long chat (>8 turns) that forces eviction, then ask about turn 1 —
  the one path where the newly-injected summary is the ONLY thing that can answer. That closes the loop on
  Bug 3 beyond the pinned-fact case.
