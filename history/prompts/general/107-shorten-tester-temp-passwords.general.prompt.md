---
id: 107
title: Shorten tester temp passwords to 6 chars
stage: general
date: 2026-07-28
surface: agent
model: claude-fable-5
feature: none
branch: release-deploy
user: Riaz Mehmood
command: none
labels: [admin, auth, usability]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files:
- src-tauri/src/api/admin.rs
- CHANGELOG_DEV.md
tests:
- none (cargo unavailable on local Windows machine; change is a safe string slice)
---

## Prompt

while creating users it is creating very long passwords so it is very much difficuilt to share that with the users, can we reduce that upto 6 characters for now?

(mid-turn clarification): we are creating ids of testers

## Response snapshot

Located the only temp-password generation site: `api/admin.rs::create_user` used
`Uuid::new_v4().simple().to_string()` — a full 32-char hex string. Truncated to
the first 6 lowercase hex chars. Verified no minimum-length validation exists in
`api/auth.rs` on login or password change, so 6-char passwords are accepted.
Added a code comment flagging the deliberate weakness as tester-only, plus a
CHANGELOG_DEV.md entry with a revisit note before external provisioning.

## Outcome

- ✅ Impact: admin-created tester accounts now get a shareable 6-char temp password.
- 🧪 Tests: none run locally (no cargo on this machine); compiles on VM at deploy.
- 📁 Files: src-tauri/src/api/admin.rs (create_user), CHANGELOG_DEV.md.
- 🔁 Next prompts: deploy to VM (cherry-pick to release) for it to take effect.
- 🧠 Reflection: entropy drop (128-bit → ~24-bit) accepted knowingly for internal testers.

## Evaluation notes (flywheel)

- Failure modes observed: none
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): n/a
