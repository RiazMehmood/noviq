---
id: 137
title: Account-bound chat history across devices
stage: green
date: 2026-08-01
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform request)
labels: [chat-history, multi-device, query_log, migration-020, F020-masking, soft-delete]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src-tauri/src/db/pg/migrations/020_chat_history_sync.sql
  - src-tauri/src/db/repositories/query_log.rs
  - src-tauri/src/api/history.rs
  - src-tauri/src/api/mod.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/api/query.rs
  - src/api/index.ts
  - src/lib/chatCache.ts
  - src/stores/auth.ts
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: PASS (clean)"
  - "vite build: PASS (built in 7.98s)"
  - "cargo check: NOT RUN — no local Rust toolchain, VM unreachable (VPN down, port 22 timeout)"
---

## Prompt

I think users chat history is bound to device only, as we sign in to other device it vanishes, i want to be available when ever i logged in in other devices too.

## Response snapshot

Diagnosis confirmed the report and found the fix was far smaller than it looked.

`ChatPage.tsx` persisted the whole history to `localStorage` only —
`noviq:conversations` (rail) and `noviq:turns:<id>` (transcripts). Browser storage
is per-origin per-profile, so a second device started empty.

**Key finding:** the server has recorded every turn since F014 in `query_log`
(`user_id`, `conversation_id`, `question`, `answer`, `citations_json`,
`created_at`), and `poll_turn` already reads it back for tunnel-timeout recovery.
The history existed server-side all along — there was no route by which the
officer who wrote it could read it. So: three endpoints and two columns, not a
new subsystem.

Two product decisions were put to the architect; both took the recommendation:
1. **Delete = soft delete** (`hidden_at`). `query_log` also backs the admin usage
   panel, activity list and per-user export; a hard delete would let any user
   punch holes in that audit trail.
2. **Masking = show placeholders** on the second device. The label→value map never
   leaves the browser (F020), so the real values *cannot* be shown elsewhere; the
   turn now states this instead of looking corrupt.

Implementation:
- **Migration 020** (additive): `response_json` (full `TurnResponse`, so a replayed
  chat keeps web results / widen button / trace handle — `citations_json` alone
  cannot rebuild it), `hidden_at`, and `idx_query_log_user_conv`.
- **`api/history.rs`**: `/api/list_conversations`, `/api/conversation_turns`,
  `/api/hide_conversation`. Every read scoped to the session's `user.id`, never a
  body field — conversation ids are client-generated, so a guessed id must not
  open another officer's chat (same invariant as `latest_since`).
- **Merge rule**: the server decides which conversations exist; the local cache
  supplies the contents of the ones it has (it holds the *unmasked* questions, so
  it is strictly richer). Side effect: one officer's cached chats can never
  surface in another's rail.
- **`clearChatCache()` on logout** — the cache holds unmasked confidential values;
  on a shared browser it would otherwise pass to the next person to sign in.
  Also consolidates three storage-key strings into one module.

Desktop (Tauri) unchanged — no session to scope by, keeps local history.

## Outcome

- ✅ Impact: chat history follows the login onto any device; deletion propagates
  everywhere while the audit trail stays intact.
- 🧪 Tests: `tsc --noEmit` clean; `vite build` clean. **Rust not compiled** — no
  local toolchain and the VM was unreachable (VPN down). `cargo check` on the VM
  is a required gate before this ships.
- 📁 Files: 6 Rust/SQL, 4 TypeScript, 1 changelog.
- 🔁 Next prompts: compile-check on the VM; deploy alongside the 12 already-staged
  `release-deploy` commits (they ship together — one push to `release` deploys all).
- 🧠 Reflection: the instinct was "build a conversation store"; reading
  `query_log` first turned a subsystem into two columns. Checking what the server
  already records should precede any "we need to persist X" design.

## Evaluation notes (flywheel)

- Failure modes observed: none at build time. Untested at runtime — the SQL
  (`DISTINCT ON` + window functions) has not executed against a live Postgres.
- Graders run and results (PASS/FAIL): tsc PASS, vite build PASS, cargo check NOT RUN.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): on the VM, `cargo check`, then probe
  the three routes unauthenticated — 401 means the route is live, 405 means it is
  absent (the reusable trick from the global-search fix).
