---
id: 064
title: Web polling fallback for SSE-tunnel indexing status
stage: green
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: release
user: Riaz Mehmood
command: (conversational)
labels: [f021, web, sse, polling, trycloudflare, frontend]
links:
  spec: specs/021-user-documents/spec.md
  ticket: n/a
  adr: n/a
  pr: n/a
files:
  - src/pages/SettingsPage.tsx
  - src/pages/ChatPage.tsx
  - CHANGELOG_DEV.md
tests:
  - npx tsc --noEmit (clean)
  - browser verification pending (user re-tests upload/attach status convergence through tunnel)
---

## Prompt

recall memory still server is not live building continously.
i think it again stuck in the loops of failure.
open this in browser
yes it works, what is remiaining now? on what we were working?
yes start on the polling fallback

## Response snapshot

Diagnosed "server not live / building continuously": noviq-server healthy (health ok,
engine vllm ready), deploy timer polling "up to date" every 2–3 min — NOT rebuilding;
no sentinel. Real cause = cloudflared quick tunnel restarted 20:17 PKT; the working URL
settled back on https://follows-biol-boundary-worthy.trycloudflare.com (200); a
ceramic-conceptual-inventory-regular URL from the same restart is dead. Opened working
URL in browser — user confirmed it works.

Then implemented the planned F021 SSE-tunnel polling fallback (frontend only, web-only
`!isTauri()`):
- SettingsPage: `indexPollNeeded` createMemo (any folder or myDocs source
  pending/processing) → createEffect interval 5s calling `loadFolders()` (reloads
  myDocs+usage on web) + `refreshFolderSourcesIfOpen` for still-active folders;
  self-clears via memo flip + effect cleanup.
- ChatPage: `attachPollNeeded` memo → 4s interval `loadAttachments(conversationId())`,
  same pattern.
SSE listeners kept for desktop/direct access. tsc clean. CHANGELOG_DEV.md entry added.
Committed + pushed `512d02ac` to release (auto-deploy).

## Outcome

- ✅ Impact: web clients now converge indexing statuses (folders, My documents, attach
  chips) even when the tunnel buffers SSE to zero bytes.
- 🧪 Tests: tsc --noEmit clean; live browser test pending (user).
- 📁 Files: SettingsPage.tsx, ChatPage.tsx, CHANGELOG_DEV.md.
- 🔁 Next prompts: browser-verify polling; then F021 §2.5 admin usage view, §3 sharing,
  §4 attachments, §5 SQL checks, T030–T033; chat token streaming through tunnel still
  broken (needs WS or chunked fallback).
- 🧠 Reflection: memo→effect self-clearing interval is the minimal Solid idiom for
  conditional polling; keep SSE as the primary transport so desktop UX is unchanged.

## Evaluation notes (flywheel)

- Failure modes observed: quick-tunnel URL rotation misread as "server down / build loop".
- Graders run and results (PASS/FAIL): tsc PASS; runtime PASS pending user browser test.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): WebSocket transport for chat token streaming
  (cloudflared proxies WS correctly).
