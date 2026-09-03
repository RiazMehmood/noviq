---
id: 063
title: F021 browser test — fixes and SSE tunnel blocker
stage: general
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: release
user: Riaz Mehmood
command: none
labels: [browser-test, upload-ux, sse, cloudflared, contrast, quota]
links:
  spec: specs/021-user-documents/spec.md
  ticket: none
  adr: none
  pr: none
files:
  - src-tauri/src/db/repositories/sources.rs
  - src-tauri/src/api/admin.rs
  - src-tauri/src/api/uploads.rs
  - src/api/transport.ts
  - src/pages/SettingsPage.tsx
  - src/components/CitationFeedback.tsx
  - CHANGELOG_DEV.md
tests:
  - live browser test quickstart §1 (PASS) + §2 (PASS after fixes); tsc clean; VM deploy builds green
---

## Prompt

Session-long F021 browser-test support (multiple messages): "still url is not working why?" /
"already two shells are running check them" / test guidance requests / "ApiError: mismatched
types; Option<i64> INT8 not compatible with NUMERIC" / "indexed color not visible… feedback card
background too" / "create the document for test 1 and guide me with questions" / §2 files request /
"rejection now clear but still showing processing and pending continuously" / "save progress
upto now, we will start working from there."

## Response snapshot

1. Diagnosed URL outage: deploy poller only saw migration-fix `b557891b` at 15:33; verified
   deploy healthy + migration 9 applied + tunnel 200. "Two shells" = url-watch.sh parent+subshell
   (benign cloudflared URL watcher).
2. Generated fictional test PDFs (private-case, notification, document_lifecycle set) and drove
   quickstart §1 (FULL PASS incl. admin exclusion) and §2 (PASS).
3. Fixed three bugs found by testing: (a) `c8964c28` PG SUM(bigint)→NUMERIC decode crash in
   usage/quota + admin usage; (b) `8df3ea3c` dark-on-light contrast for indexed/failed badges +
   tinted feedback card; (c) `3c455641` upload UX — per-file accept/reject (UploadOutcome with
   rejected[{file_name,reason}]), partial batch acceptance under quota, amber rejection panel,
   friendly error messages, myDocs live status flip.
4. Proved the remaining "stuck processing" is transport-level: SSE streams perfectly on
   localhost but the trycloudflare quick tunnel delivers 0 body bytes forever (headers only);
   affects /api/index/events AND /api/chat/events (web chat never actually streamed).
   `--protocol http2` no help; tunnel URL rotated. Planned fix (next session): web polling
   fallback in SettingsPage + ChatPage attach chips.

## Outcome

- ✅ Impact: §1+§2 verified; three shipped fixes; SSE root cause isolated with reproducible curl evidence.
- 🧪 Tests: live browser + prod DB asserts; tsc 0; VM incremental builds green.
- 📁 Files: see files list; memory updated with full resume plan.
- 🔁 Next prompts: implement polling fallback → §2.5 admin usage → §3 sharing → §4 attachments → §5 SQL → T030-T033.
- 🧠 Reflection: verify event transports end-to-end through the real proxy, not just server-side; PG aggregate types need casts.

## Evaluation notes (flywheel)

- Failure modes observed: silent SSE buffering at CDN edge masquerading as FE bug; PG aggregate type mismatch only surfacing at runtime.
- Graders run and results (PASS/FAIL): §1 PASS, §2 PASS post-fix, SSE-via-tunnel FAIL (0 bytes).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): polling fallback; longer-term WebSockets (cloudflared proxies WS).
