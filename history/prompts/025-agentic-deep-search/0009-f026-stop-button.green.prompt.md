---
id: 0009
title: F026 stop button — cancel a wrong / slow chat turn
stage: green
date: 2026-07-15
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [backend, frontend, scheduler, cancellation, deep-search, ux]
links:
  spec:
  ticket:
  adr: (suggested) stateful-deep-search-checkpointing
  pr:
files:
  - src-tauri/src/services/scheduler.rs
  - src-tauri/src/services/assistant/capabilities/guidance.rs
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/commands/query.rs
  - src-tauri/src/lib.rs
  - src/api/assistant.ts
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "npx tsc --noEmit -p . (frontend, GREEN)"
  - "scheduler.rs: 2 new unit tests (cancel_conversation, stale clear) — compiled on VM (pending)"
---

## Prompt

"design like langraph memory carry over check points, resume time travel, persistence, and there is no stop
button during query if user write wrongly and ask wrong query so he can stop, and other thing is guardrails for
test, single word a, or greetings etc." → then "ok" (proceed with the recommended first step = the stop button).

## Response snapshot

Designed the full F026 (3 parts) grounded in the code, then built **Part 2 (the stop button)** because the
cancellation engine already existed and it's the highest daily value.

Key recon finding: the primitives were already present — `Scheduler::request_cancel(Uuid)`, a live
`CancellationToken` per workload, cooperative `cancelled()` checks at every deep-loop boundary, and the
guardrails (`is_low_signal` catches `a`/≤2 chars/digits/probes, `is_greeting_or_intro` catches greetings) that
already short-circuit BEFORE retrieval. The only missing links for Stop were a conversation→workload-id map and
a UI trigger.

Built:
- `scheduler.rs`: `chat_turns: DashMap<String,Uuid>` + `note_chat_turn` / `clear_chat_turn` (removes only if the
  id still matches, so a slow tail turn can't wipe a newer turn's mapping) / `cancel_conversation`. 2 unit tests.
- `guidance.rs`: note/clear the mapping around `register`/`unregister` (the normal + deep Q&A path).
- `POST /api/cancel_turn` (+ router) and a desktop `cancel_turn` Tauri command (+ invoke-handler registration).
- `assistant.ts` `cancelTurn` wrapper; `ChatPage.tsx` send arrow → red Stop (■) while `busy()`, a per-turn
  `stoppedThisTurn` flag renders a clean `_Stopped._` for both the empty-answer and dropped-connection paths and
  skips tunnel recovery.

## Outcome

- ✅ Impact: an officer can halt a wrongly-typed or slow (deep ~135s) query; it unwinds at the next cooperative
  boundary (~1–3s) with a clean "Stopped." and the composer re-enables. No wasted full run.
- 🧪 Tests: frontend `tsc --noEmit` GREEN; 2 new scheduler unit tests added (compile on the VM pending — Windows
  can't do the CUDA build locally). `dashmap 6.1 remove_if` verified.
- 📁 Files: scheduler.rs, guidance.rs, api/query.rs, api/router.rs, commands/query.rs, lib.rs, api/assistant.ts,
  ChatPage.tsx, CHANGELOG_DEV.md.
- 🔁 Next prompts: VM `cargo build --release --features llm,server,cuda` → deploy → smoke (deep query → Stop →
  halts + "Stopped"). Then Part 1 (persisted `deep_checkpoint` + resume + time-travel) and Part 3 (deep
  cost-guard nudge). PHR after deploy; ADR `/sp.adr stateful-deep-search-checkpointing`.
- 🧠 Reflection: read the scheduler + compat + guidance first — the "no stop button" was a *wiring* gap, not a
  missing engine, so the diff stayed small (one map + one endpoint + one button) instead of new cancellation
  infra.

## Evaluation notes (flywheel)

- Failure modes observed: none yet (frontend build-verified only; backend build is the user's GPU step).
- Graders run and results (PASS/FAIL): tsc PASS. Backend unit tests pending VM compile.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): after deploy, confirm Stop halts a live deep run at a round boundary
  and that transform/mapreduce (not yet in the conversation map) is an acceptable v1 gap.
