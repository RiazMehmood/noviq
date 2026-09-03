---
id: 115
title: Retrieval pool 80/20 split
stage: misc
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [retrieval, ops, env-knobs]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- CHANGELOG_DEV.md
tests:
- verified ECHO_VEC_K=28/ECHO_FTS_K=7 in the running process environ; /api/health ok
---

## Prompt

currently we are using 70/30 percentage of vec & fts5, now i want it to be
80/20 do that.

## Response snapshot

The "70/30" is the hybrid-union pool split ECHO_VEC_K=25 / ECHO_FTS_K=10
(25/35 ≈ 71/29). Kept total depth 35 and set 80/20 = 28/7 in
/opt/noviq/echoAI/.env via sed (no secrets displayed), restarted noviq-server
with the sudo password-file redirect workaround, verified the new values in
/proc/<pid>/environ and health ok. Password scratch file deleted after use.
No code change; CHANGELOG entry added with rollback values.

## Outcome

- ✅ Impact: live retrieval pool now 28 vector + 7 FTS chunks per query.
- 🧪 Tests: process environ shows 28/7; health endpoint ok, engine ready.
- 📁 Files: VM /opt/noviq/echoAI/.env (out of repo); CHANGELOG_DEV.md.
- 🔁 Next prompts: watch retrieval quality; rollback = 25/10 + restart.
- 🧠 Reflection: "70/30" mapped to pool-depth knobs, not a score weight — confirm the mechanism before changing numbers.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
