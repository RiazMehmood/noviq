---
id: 133
title: Web search — the toggle decides the turn, the answer is shaped, the lane remembers
stage: green
date: 2026-07-31
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: "ok then fix this also, so it must contain previous chat history also"
labels: [websearch, routing, conversation-memory, ui, deployed]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/services/websearch/mod.rs
  - src-tauri/src/services/query_router.rs
  - src/components/AnswerBody.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "cargo test --lib --no-default-features --features server: 850 passed, 0 failed"
  - "guidance::tests::a_followup_carries_its_subject_into_the_web_search"
  - "guidance::tests::a_self_contained_question_does_not_carry_the_previous_subject"
  - "guidance::tests::a_followup_web_prompt_carries_the_earlier_turns"
  - "guidance::tests::last_user_question_reads_the_most_recent_one"
  - "websearch::tests::web_only_prompt_asks_for_a_shaped_answer"
  - "tsc --noEmit clean; vite build clean"
  - "DEPLOYED to prod: be86f41 then 0cbe5aa; health ok; NOT exercised in a real turn"
---

## Prompt

ok then fix this also, so it must contain previous chat history also

(Same thread: "what are new amendments occured in sindh public procurements
rules in 2024?" answered from the web, while the near-identical "what are new
amendments in sindh public procurement rules in 2024" returned "No amendments
are recorded … in the indexed documents". Then: "it is simple if web search is
on it must answer from that, and still format after web search answer is not
good" · "only commit this change only to live" · "what is conversational memory
during web search does it exist or not?" · "deploy this one also".)

## Response snapshot

Diagnosed from code, not notes: `amendments in` matched a Timeline cue routing to
`AmendmentChain` — a lane reading neither corpus nor web — and dispatch fed it
the WHOLE QUESTION as the provision to look up.

Made the toggle outrank classification, stopped the provision lookup firing
without a provision, shaped the web answer, made `[W]` markers clickable, split
cited sources from the rest — then, when asked, found the web lane had never
carried conversation history at all and fixed both halves of that (the search
query and the prompt).

Shipped both commits to prod on request, one at a time.

## Outcome

- ✅ Impact: a toggle that was honoured on one route out of many now decides the
  turn; follow-ups keep their subject; the web answer is headed and itemised with
  clickable sources.
- 🧪 Tests: 850 green, five new.
- 📁 Files: 6 source + changelog.
- 🔁 Next prompts: exercise it live (question → bare follow-up); iterate the
  answer shape if needed; `AmendmentChain` is dead (`amendment_events` empty) and
  its Timeline cue is still loose for web-off turns.
- 🧠 Reflection: **fix 1 made fix 2 necessary.** Routing every web turn into
  `guidance` exposed that this lane forgot the conversation — a gap that had
  affected some queries now affected all. Widening a lane without first checking
  what that lane drops is the error; the user found it by asking, not me.

## Evaluation notes (flywheel)

- Failure modes observed: (a) two near-identical phrasings taking opposite routes
  because of one intervening word; (b) a refusal claiming the indexed documents
  had been checked by a lane that never reads them; (c) my own fix widening a
  regression before it was noticed.
- Graders run and results (PASS/FAIL): PASS on unit tests; **live behaviour
  UNVERIFIED** — the deployed change has not been through a real turn.
- Prompt variant (if applicable): WEB_ONLY_SYSTEM_PROMPT gained a SHAPE section
  (lead sentence, bold heading, bulleted list, define instruments on first use).
- Next experiment (smallest change to try): one real web turn plus a bare
  follow-up, and read the trace step "Web conversation memory" to confirm the
  prior-turns block is injected and the search subject carried.
