---
id: 008
title: Admin portal for beta user management
stage: spec
date: 2026-06-25
surface: agent
model: claude-opus-4-8
feature: 014-server-thin-client
branch: 015-bureaucrat-chat
user: engr.riaz2010@gmail.com
command: (freeform amendment)
labels: [spec, admin, users, beta, rbac-seam]
links:
  spec: specs/014-server-thin-client/spec.md
  ticket:
  adr: history/adr/0017-server-deployment-and-update-strategy.md
  pr:
files:
  - specs/014-server-thin-client/spec.md
  - specs/014-server-thin-client/data-model.md
  - specs/014-server-thin-client/contracts/http-api.md
  - specs/014-server-thin-client/tasks.md
  - src-tauri/src/db/pg/migrations/005_server_users_sessions.sql
tests:
  - none (spec amendment; US7 tasks T043–T048 added, depend on US2)
---

## Prompt

i also need an admin portal from where i can create more users also for beta testing also

## Response snapshot

Scoped via two decisions (env-seeded first admin; portal = create/list/disable). Folded into 014 as
US7 (P2): FR-019 admin bootstrap from NOVIQ_ADMIN_USER/PASS, FR-020 admin-only role guard (403),
FR-021 user CRUD endpoints (`/api/admin/users` + disable/enable), FR-022 admin portal page; SC-009/010
added. Updated data-model (users.role enforced + disabled_at; corrected TEXT/bigint to match the
real schema), contracts (admin endpoint table), and tasks (Phase 8b T043–T048, depends on US2;
renumbered Polish to T049–T052, gate now covers SC-001..010). Edited the not-yet-applied migration
005 to add `role NOT NULL DEFAULT 'user'` + `disabled_at`. US7 sequenced after US2 (needs auth);
full RBAC (departments/per-doc ACL/password-reset/role-edit) stays the follow-on.

## Outcome

- ✅ Impact: beta onboarding/offboarding now specced without DB/SSH; reuses the migration-005 role/disabled seam.
- 🧪 Tests: none — spec stage. T048 covers the 403 + disabled-login gate.
- 📁 Files: spec.md, data-model.md, contracts/http-api.md, tasks.md, migration 005.
- 🔁 Next prompts: implement US2 (T016–T021) then US7 (T043–T048); US7 can't precede auth.
- 🧠 Reflection: the reserved `role` column from T005 meant the admin portal was a small additive amendment, not a schema rework.

## Evaluation notes (flywheel)

- Failure modes observed: data-model.md had uuid/timestamptz that mismatched the real TEXT/bigint schema; corrected during the amendment.
- Graders run and results (PASS/FAIL): n/a (spec).
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): implement US2 auth first, then US7 on top; smoke admin-seed on a scratch data dir.
</content>
