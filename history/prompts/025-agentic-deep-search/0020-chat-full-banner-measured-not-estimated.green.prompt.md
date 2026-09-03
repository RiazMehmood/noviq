---
id: 0020
title: Chat-full banner measured, not estimated
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [context-budget, ux, fr-018a, conversation-memory, measurement]
links:
  spec: specs/015-bureaucrat-chat/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/query_router.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/assistant/capabilities/{draft,count,amendment_chain,mod,summarize_doc,transform}.rs
  - src-tauri/src/services/assistant/capabilities/notice_assist/mod.rs
  - src-tauri/src/services/deep_search/mod.rs
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "vm: cargo check --lib --no-default-features --features llm,server — exit 0"
  - "vm: cargo test --lib ... guidance:: — 9/9 PASS (2 new)"
  - "guidance.rs::fill_notice_follows_measured_chat_fullness_not_the_prompt"
  - "guidance.rs::measured_fullness_overrides_the_post_hoc_estimate"
  - "frontend: tsc --noEmit clean; npm run build exit 0"
---

## Prompt

first on immediate base we need to tell user its window is full. as currently users are testing the
product so no document found and continueing chat is not good impression first we need to fix this
window option when it fulls shows banner that this chat window is full, and we only dont need to
carry over grade, length of servic and leave only these were just examples i shared there could be
anything else

(then: "yes go ahead fix the window full banner" → "commit and deploy it")

## Response snapshot

### The finding: the notice has never fired for anyone
FR-018a's fresh-chat notice already existed (`context_fill_threshold = 0.80`, banner in ChatPage) —
it just never showed. `guidance.rs:291` built its fill number from **`qr.citations` excerpts** (the ~9
passages the model QUOTED) instead of the ~33 chunks it READ ⇒ ~35% when the real prompt is at 94% ⇒
never crosses 0.80. Dead feature. Same disease as the week's other bugs: **two places measuring one
thing; the wrong one is on screen.** (`plan_budget` is the post-hoc audit that also lied about
`summary_included`.)

### The trap avoided
"Just use the real prompt size" is ALSO wrong: the prompt is ~always ~100% full by construction (law
expands to fill the leftover). Prod: 94/95/99/101/96/98% across six turns. That bar would warn on turn
one forever. **Fullness must measure the non-law OVERHEAD** against the point where the law budget
floors (`gen_max_prompt − CONTEXT_TOKEN_BUDGET` = 4512): 35% turn one → 77% at ten exchanges → 100%
at ~fifteen, where truncation starts eating the law. The existing 0.80 fires at ~10 exchanges — the
number the architect had independently picked by instinct.

### Shipped
`ChatFill { used, limit }` on `QueryResponse` (serde-default), computed once beside the measured
`law_overhead` and returned on **every** exit incl. refusal paths (a refusal in a full chat is exactly
when you must say so). `AssembleInput.chat_fill` (measured wins, plan is fallback). ChatPage: notice
moved out of the answer body into ONE live composer banner — amber ≥80%, **red + submit blocked at
100%** (slash commands exempt). **Deleted "(your case facts carry over)"** — a lie twice over: facts
are per-conversation AND not injected at all.

### On "facts could be anything" (architect's correction)
The 3 regexes (grade/service/leave) are examples, not the memory. Only two honest ways to carry
"anything": the **summary** (catches anything, lossy, already built) or **LLM fact-extraction**
(catches anything, can confabulate — unacceptable for legal work). ⇒ the summary IS the "anything"
mechanism and becomes load-bearing for carry-over; the regex facts are a precision tool for the few
patterns detectable without guessing. Recorded as a follow-up, not built.

### Three self-inflicted process failures (all "trusted a pattern, didn't check the result")
1. Regex bulk-edit over `QueryResponse \{` matched fn signatures + the struct def → injected a field
   into a fn body, left a mangled `hat_fill`. Reverted both files, redone by hand with asserted-unique
   anchors.
2. Built the `AssembleInput` file list from a grep for `context_fill` (finds *consumers*) → missed
   `transform.rs` + `summarize_doc.rs`, which construct it and never mention the field. Compiler
   caught it. **Grep for the constructor, not a field.**
3. Bash cwd persists between calls → an `scp src-tauri/...` sent nothing and the rebuild ran on stale
   files. Fixed with absolute paths + md5-verifying every VM copy against local before committing.

## Outcome

- ✅ Impact: the chat-full warning works for the first time; users get an honest amber at ~10
  exchanges and a hard stop where answers would otherwise become refusals. One product lie removed.
- 🧪 Tests: VM `cargo check` exit 0; `guidance::` 9/9 incl. 2 new; tsc clean; `npm run build` exit 0;
  all 11 Rust files md5-verified identical to what compiled.
- 📁 Files: 11 Rust + ChatPage.tsx + CHANGELOG.
- 🔁 Next prompts: (1) **browser-verify** — ~10 questions in one chat, watch amber then red;
  (2) build "Continue in a new chat" carry-over (summary + facts) so the red banner has somewhere to
  send people; (3) deep search still reports the old estimate; (4) deep cost-guard decision;
  (5) resume graph-history.
- 🧠 Reflection: the valuable part was refusing the obvious fix twice — first the dead estimate, then
  "use the real prompt size", which would have been equally broken. The right metric fell out of
  asking *what actually degrades*, not *what number is available*.

## Evaluation notes (flywheel)

- Failure modes observed: three bulk-edit/pattern-trust failures in one session (above). Cheap
  (compiler-caught) but repeated — the lesson is verify the RESULT, not the pattern.
- Graders run and results (PASS/FAIL): cargo check PASS; guidance:: 9/9 PASS; tsc PASS; fe build PASS.
  Browser click-through NOT RUN.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): one long chat in the browser; confirm the banner's % and
  the trace's "Prompt overhead (measured)" agree — they now share one source.
