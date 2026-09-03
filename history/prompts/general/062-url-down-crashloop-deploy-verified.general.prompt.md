---
id: 062
title: URL down — crashloop deploy verified healed
stage: general
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: release
user: Riaz Mehmood
command: none
labels: [ops, deploy, migration, crashloop, vm]
links:
  spec: specs/021-user-documents/spec.md
  ticket: none
  adr: none
  pr: none
files:
  - none (ops/diagnosis only; memory files updated outside repo)
tests:
  - live checks: /api/health ok; _sqlx_migrations v9 success=t; tunnel HTTP 200
---

## Prompt

still url is not working why?

(follow-up mid-task): already two shells are running check them also what they are running for?

## Response snapshot

Diagnosed why the public URL was still down after the migration-009 fix was pushed last
session. Root cause of continued outage: the VM deploy poller kept reporting "up to date"
at old commit `bf3e82f9` — the fix `b557891b` only became visible to the VM at 15:33 PKT
today, so the server crashlooped ~800 restarts on `column "visibility" of relation
"sources" already exists` until then. Deploy timer then pulled + rebuilt (CUDA release
build); server restarted cleanly. Verified end-to-end: `/api/health` ok (gpu:true, vllm
ready), `_sqlx_migrations` version 9 "sources ownership" success=t, `sources.visibility`
column present, public quick-tunnel URL returns HTTP 200.

Two shells question: both are `bash /opt/noviq/url-watch.sh` (parent + its pipeline
subshell), root-owned since Jun 26 — a watcher that syncs the live cloudflared
quick-tunnel URL into `/opt/noviq/current-url.txt`. Benign; left running.

Memory files updated (F021 project note + MEMORY.md index) to record deploy verified.

## Outcome

- ✅ Impact: production URL restored and verified; F021 deploy blocker closed.
- 🧪 Tests: live prod checks (health endpoint, psql migration row, tunnel HTTP 200).
- 📁 Files: no repo changes; memory notes updated.
- 🔁 Next prompts: user browser-tests quickstart §1–§4 (needs 2 accounts); T030-T033; branch cleanup.
- 🧠 Reflection: "fix pushed" ≠ "fix deployed" — always confirm the deploy poller saw the new commit before declaring healed.

## Evaluation notes (flywheel)

- Failure modes observed: deploy poller lag masked as persistent crashloop; systemd restart counter (800+) was the tell.
- Graders run and results (PASS/FAIL): PASS (health + migration + tunnel all green).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): consider deploy.sh alerting when restart counter exceeds threshold.
