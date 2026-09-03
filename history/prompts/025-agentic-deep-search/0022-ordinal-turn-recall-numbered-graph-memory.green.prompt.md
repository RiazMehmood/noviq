---
id: 0022
title: Ordinal turn-recall (numbered graph-memory tier 2)
stage: green
date: 2026-07-17
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: engr.riaz2010@gmail.com
command: build the simple numbered version first
labels: [conversation-memory, ordinal-recall, graph-memory, budget-seam]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/memory.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - CHANGELOG_DEV.md
tests:
  - "assistant::memory (lib): 17/17 (6 new ordinal tests)"
  - "query_router (lib): 71/71 regression green"
  - "cargo check --features llm,server: exit 0"
---

## Prompt

build the simple numbered version first

(Context: after replaying the architect's 8-turn chat on prod `5783001b` and confirming the
chat-full banner is dormant by design, the discussion turned to why Q6 "what was my first
question?" still named the wrong turn. The architect's GRAPH.png design — numbered turn nodes
1..n, each holding question/answer/citations, looked up on demand — was chosen. The user
clarified this is WITHIN a single chat (no cross-chat linking) and asked to build the simple
numbered version first.)

## Response snapshot

Implemented graph-memory **tier 2**: a deterministic `seq` lookup, no model in the path.

- `memory.rs::ordinal_recall_block(pool, conversation_id, query)` — `parse_ordinal_refs`
  matches an ordinal token (`first`..`tenth`, `1st`..`10th`, `last`/`previous`/`latest`)
  adjacent to a turn noun (`question`→user, `answer`/`reply`/`response`→assistant,
  `one`/`thing`→exchange). A legal ordinal with a non-turn noun ("first appeal", "first
  schedule") does not match. Reads the FULL turn history incl. `summarized = 1` rows, excludes
  the current in-flight user turn from the question list, quotes each resolved turn (300-char
  cap), renders a non-citable `=== EARLIER TURNS IN THIS CONVERSATION (recall) ===` block.
  Empty when no ordinal referenced (zero cost). Fixes Q6 and Q7.
- `guidance.rs::run` — appends the block to `sys` (alongside `ATTACHMENT_SCOPE_OVERRIDE`),
  which `route_scoped`'s `law_overhead` MEASURES (`est_tokens(system_prompt)`, :861), so the
  law budget shrinks to fit — no overflow. This is why NOT `prior_turns_block` (pure fn over
  the capped history, no DB, no evicted turns) and why the measured-`sys` seam is safe now
  (the unmeasured `memory_block` injection was reverted in `80048b37`; `9f40d4aa` added the
  measurement). Reaches deep + single-pass; zero signature churn.

## Outcome

- ✅ Impact: ordinal turn references ("my first question", "compare the first one with the
  third one") resolve to the exact earlier turn even after eviction, instead of the oldest
  visible turn. Unbounded reach at bounded cost.
- 🧪 Tests: `assistant::memory` 17/17 (6 new); `query_router` 71/71; `cargo check` clean —
  all on a VM worktree at `5783001b`, `--features llm,server`.
- 📁 Files: memory.rs (+lookup +6 tests), guidance.rs (wiring), CHANGELOG_DEV.md.
- 🔁 Next prompts: deploy + replay the 8 questions in a fresh browser chat; confirm Q6 → real
  Q1 and law didn't collapse (budget stayed measured). Then consider turn→law graph edges and
  re-landing pinned-facts/summary via the same seam.
- 🧠 Reflection: the store already existed (every turn in `chat_message` with a dense seq);
  only the lookup was missing. The whole risk was the injection seam, not the query.

## Evaluation notes (flywheel)

- Failure modes observed: prior to fix, Q6 named the oldest *visible* turn (Q3), not Q1.
- Graders run and results (PASS/FAIL): unit PASS (17/17 memory, 71/71 query_router).
- Prompt variant (if applicable): n/a (deterministic, non-LLM recall).
- Next experiment (smallest change to try): live trace replay to confirm the recall block
  appears in the system prompt and `law_overhead` rose only by the block's tokens.
