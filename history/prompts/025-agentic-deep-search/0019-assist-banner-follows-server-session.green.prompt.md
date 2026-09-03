---
id: 0019
title: Assist banner follows the server session, not a stale mirror
stage: green
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: 025-agentic-deep-search
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: (chat)
labels: [notice-assist, state-sync, frontend, api, not-compile-verified]
links:
  spec: specs/022-notice-assist/spec.md
  ticket:
  adr:
  pr:
files:
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/router.rs
  - src/api/assistant.ts
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "frontend: npx tsc --noEmit — PASS"
  - "vm: cargo check --lib --no-default-features --features llm,server — exit 0 (3m54s)"
  - "vm: cargo test --lib ... persona:: — 7/7 PASS (incl. 2 new guardrail tests)"
  - "vm: cargo test --lib ... intent::tests — 27/27 PASS"
  - "vm: smoke test vs shadow server on isolated PG (:8099) — 9/9 PASS"
---

## Prompt

ok work on desync

(preceded by: "vpn is up now. what is deep cost guard and reload desync? simply explain it")

## Response snapshot

**My earlier description of this bug was WRONG, and reading the code before building caught it.**

I had told the user a *page reload* strands them in assist mode. It does not: `ChatPage.onMount`
always mints a **fresh** conversation id ("every launch starts on a blank New chat"), so F5 lands in
a NEW conversation where the hidden banner is correct. Corrected in CHANGELOG_DEV.md rather than
left standing.

The real defect is in the two paths that **switch** conversations, where `assistMode` was never
updated:
- `newChat()` (ChatPage.tsx:871) reset turns/attachments/streaming but NOT `assistMode` → the banner
  followed the officer into a brand-new chat with no session (**false positive** — banner lies).
- `selectConversation()` (:883) never set `assistMode` at all → reopening a chat where `/help` was
  typed showed NO banner while the session was still open, so every question stayed routed into the
  notice flow (**false negative** — the dangerous one; assist mode with no sign of it). Reachable
  from the history list, which is where you land after F5 anyway.

Fix — ask the server instead of guessing:
- `POST /api/assist_status` `{conversation_id}` → `{open: bool}`; one indexed PK lookup, no model,
  no retrieval. Sibling of the `assist_exit` endpoint from PHR 0018.
- `router.rs` route; `assistant.ts` `assistStatus()`.
- `ChatPage.tsx` `refreshAssistMode(id)`. Both switch paths clear the banner first (the previous
  conversation's value is *always* wrong for the new one); `selectConversation` then corrects it
  upward from the server. `newChat` skips the round trip — a fresh UUID provably has no session.
  Race guard: `if (conversationId() === id)` so a slow reply can't clobber a conversation the
  officer has since left.
- On a failed status read: banner stays hidden + `console.warn` (not swallowed). Residual noted.

## Outcome

- ✅ Impact: the assist banner now reflects the server session on every conversation switch, closing
  both the false-positive (stale banner in a new chat) and the false-negative (invisible assist mode
  in a reopened chat).
- 🧪 Tests: **ALL GREEN on the VM** (user authorised the staging worktree `/opt/noviq/f0018-check`
  @ `9a0faccb`): `cargo check` exit 0 (3m54s); `persona::` 7/7 (both new guardrail tests pass);
  `intent::tests` 27/27. No warning names a changed file. Frontend `tsc --noEmit` PASS. Only pure
  tests run — a second embedded PG can kill prod's DB on drop, and none of these need one.
  Prod untouched: `release @ 9f40d4a`, `noviq-server` active, `f025-wt` untouched.
  **NOT smoke-tested against a running server; NOT deployed; uncommitted.**
- 📁 Files: 4 changed + CHANGELOG_DEV.md. Nothing committed.
- 🔬 Smoke test (added after the user authorised it): **9/9 PASS** against a real `noviq-server`
  built from this diff, on a **fully isolated** PG. Key results: auth 401 without a token; the
  `conversationId` camelCase alias is accepted; `/help` → `assist_status {"open":true}` (proves the
  desync fix); `assist_exit` → `{"left":true}` then `{"left":false}` (idempotent); and `poll_turn`
  shows the latest turn is still `/help` — **`assist_exit` records NO chat turn**, the direct proof
  of the original complaint being fixed.
  Isolation: `NOVIQ_DATA_DIR=/opt/noviq/tmp/smoke-data` + `NOVIQ_BIND=127.0.0.1:8099` +
  `NOVIQ_ADMIN_USER/PASS` throwaway admin. The "never run a 2nd server" rule is about the *shared
  data dir*; a different `NOVIQ_DATA_DIR` gets its own engine. Prod postmaster pid 1089612 unchanged
  before/during/after, unit active, `/api/health` 200, `release @ 9f40d4a`, `f025-wt` bdf3ed9.
  Torn down; `/opt/noviq/f0018-check` (37M) + `/opt/noviq/f0018-target` (3.4G) kept for future checks.
- 🔁 Next prompts: (1) browser-verify the A→B→A banner (only HTTP was exercised); (2) decide the
  deep cost-guard; (3) resume graph-history; (4) drop the staging worktree/target when done.
- 🧠 Reflection: I asserted the reload bug twice — in prose and in a CHANGELOG entry — before
  reading `onMount`. The mechanism was wrong and the fix would have been aimed at the wrong trigger.
  Reading first turned a plausible story into the two real cases. State the mechanism only after
  reading it.

## Evaluation notes (flywheel)

- Failure modes observed: **confident wrong diagnosis** ("reload strands you in assist mode") that
  survived one full turn because it was plausible and nobody had read `onMount`. Caught only by
  verifying before implementing.
- Graders run and results (PASS/FAIL): tsc PASS; cargo NOT RUN.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): in a browser, type `/help` in chat A, switch to chat B,
  switch back to A — the banner must reappear (it comes from `assist_status`, not memory).
