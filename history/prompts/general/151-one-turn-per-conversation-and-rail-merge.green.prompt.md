---
id: 151
title: One turn per conversation, and stop the rail erasing the generating chat
stage: green
date: 2026-08-11
surface: agent
model: claude-opus-5
feature: general
branch: 027-my-projects
user: Riaz Mehmood
command: (conversational)
labels: [chat, ux, regression, deploy]
links:
  spec: n/a
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/stores/chat.ts
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - "tsc --noEmit: PASS"
  - "vite build: PASS"
  - "deploy verified: served bundle index-CRHDGPiD.js == local build hash"
---

## Prompt

as recently i moved the tab during the query it remains loading but in same chat
window where i send the query and which were loading, i just clicked on previous
chats then come back to that query i did not found that and also i try to start
new chat but unable to start that also

## Response snapshot

Two regressions from the same morning's store change, diagnosed by reading the
shipped code rather than reasoning from memory.

1. **The generating chat vanished from the rail.** `syncConversations` did
   `setConversations(rows.map(…))` — a wholesale replace with the server's
   history. The server lists a conversation only once a turn LANDS in
   `query_log`, so a chat whose question is still generating is absent from
   `rows`; remounting erased its rail entry and left nothing to click back to.
   The answer was never lost (it sits in the store's queue keyed by
   conversation) but was unreachable. Now merges, keeping local-only entries and
   re-sorting by recency. Latent for months — remounting into a live turn is
   what exposed it.

2. **"New chat" unusable.** The store held ONE active turn, so `busy()` was
   global and `disabled={busy()}` froze the composer in every conversation.
   Worse, `runTurn` early-returned while busy, so a second send would have
   appended the question and produced no answer and no error. Store re-keyed by
   conversation: `busyIn(id)` / `streamingIn(id)` / `phaseIn(id)` /
   `stopTurn(id)`, `stopped` as a Set, `resumeConversationId()` for the
   mount-time reopen. No global "is anything running" is exported — that shape
   caused the bug, so it is not available to reach for again. A second turn in
   the SAME conversation is still refused (two answers racing one transcript);
   another chat is allowed and queues server-side on `gen_gate`.

Deployed as `98d4a4fb` (frontend=1 backend=0, correctly). Verified by hash
match: served `index-CRHDGPiD.js` is byte-identical to the local build.

## Outcome

- ✅ Impact: a running answer stays reachable, and other chats stay usable while
  it generates.
- 🧪 Tests: typecheck + build pass; deploy verified by local/served bundle hash
  equality rather than by the deploy log.
- 📁 Files: 2 frontend files + changelog.
- 🔁 Next prompts: hydrate an answer that completed while its chat was closed
  (and the page-reload case); empty "New chat" rows; device-check responsive;
  Build Summaries.
- 🧠 Reflection: I introduced a global flag where the domain is per-conversation,
  and the API shape made the wrong thing easy. Removing the global accessor
  entirely — not just fixing its callers — is what stops it recurring.

## Evaluation notes (flywheel)

- Failure modes observed: hoisting state without re-scoping its granularity;
  a "replace" where the remote list is authoritative for only PART of the set.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): drive two concurrent chats in one
  browser and confirm both transcripts land in the right place.
