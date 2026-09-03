---
id: 0023
title: Conversation-recall deterministic short-circuit (fix for 0022)
stage: green
date: 2026-07-17
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: engr.riaz2010@gmail.com
command: again same issue, check the traces and audit it comprehensively
labels: [conversation-memory, ordinal-recall, grounding, deterministic, trace-audit]
links:
  spec: specs/025-agentic-deep-search/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/services/assistant/memory.rs
  - src-tauri/src/services/assistant/mod.rs
  - CHANGELOG_DEV.md
tests:
  - "assistant::memory (lib): 20/20 (3 new: is_pure_recall + 2 direct_recall)"
  - "cargo check --features llm,server --bin noviq-server: exit 0"
  - "LIVE: deployed e01d1ff3; Q6 replay -> recall path, 3 ms, correct Q1"
---

## Prompt

again same issue, check the traces and audit it comprehensively and compare with previous questions also.

## Response snapshot

Audited all 9 post-deploy traces of `9b990879`. The v1 (0022) system-prompt injection landed
cleanly (recall block present, overhead measured 1111→1207, no overflow) but the model **ignored
it**: Q6 "what was my first question?" → **refused** (grounding's refuse-if-not-in-documents rule
outranks a system-prompt note); Q7 "compare the first one with third one" → compared **documents**,
reading "first/third one" as retrieved passages. Q1–Q5 all correct (RC1–RC4 fixes holding).

Root cause: a strongly-grounded RAG model must not be asked to recall conversation structure —
prompt-injection is the wrong tool. Fix: **deterministic short-circuit** before capability dispatch.

- `memory.rs::direct_recall_answer` + `is_pure_recall` — pure recall = ordinal turn-ref + a
  `my`/`your`/`our` anchor + no compare/summarise verb; quotes the exact turn(s) from
  `chat_message`. Shared `fetch_turn_lists` helper (also refactored into `ordinal_recall_block`).
- `mod.rs::run_turn` — new intercept in the `greeting`/low-signal/`count` chain returning
  `TurnResponse { capability: "recall", citations: [] }`; existing tail persists it. No retrieval,
  no model, no refusal possible (shape of the deterministic Count capability, ADR-0019).

## Outcome (DEPLOYED `e01d1ff3` + LIVE-VERIFIED 2026-07-17)

- ✅ Impact: "what was my first question?" answered deterministically from the DB — **live: 3 ms,
  `recall` path, correct Q1, no retrieval, no refusal** (was a refusal on `9b990879`). Q1–Q5
  unchanged.
- 🧪 Tests: `assistant::memory` 20/20 (3 new); `cargo check --bin noviq-server` exit 0 (VM worktree
  at `9b990879`, `--features llm,server`). Live prod-trace confirmation of the `recall` short-circuit.
- 📁 Files: memory.rs (short-circuit + gate + shared helper), mod.rs (intercept), CHANGELOG_DEV.md.
- 🔁 Next prompts: none required — feature closed. Optional future: compound compare-over-turns (Q7).
- 🧠 Reflection: the trace audit again beat reasoning-from-code — the v1 injection "worked" by every
  measurable signal (present, measured, no overflow) yet the model still refused. Behaviour, not
  presence, is the test.

## Evaluation notes (flywheel)

- Failure modes observed: grounding refusal beats system-prompt recall; retrieved passages captured
  the "first/third one" reference in a compound compare.
- Graders run and results (PASS/FAIL): unit PASS (20/20 memory). **Live replay PASS** — Q6 3 ms
  recall path with correct Q1; Q7 compared documents (accepted known limit); reworded Q8 ("compare
  my first question in this chat…") compared the real turns correctly.
- Prompt variant (if applicable): n/a (deterministic path, no LLM).
- Next experiment (smallest change to try): a dedicated compare-over-turns path for Q7 if ever
  needed — suppress retrieval, feed the two turns to the model for comparison only. User accepted
  the current behaviour (clarify the query), so not scheduled.
