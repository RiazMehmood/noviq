---
id: 0008
title: Deploy F027 to production with the gate closed
stage: misc
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: (freeform) "deploy it with the gate closed"
labels: [f027, deploy, production, migration-023, gate, ops]
links:
  spec: specs/027-my-projects/spec.md
  ticket: n/a
  adr: history/adr/0028-copy-on-share-and-cross-user-content-linking.md, history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  pr: pushed directly to `release` (fast-forward f1026525 -> bbcd2970)
files:
  - CHANGELOG_DEV.md
  - specs/027-my-projects/tasks.md
tests:
  - "prod: _sqlx_migrations 22 and 23 both success = t"
  - "prod: tables project, project_source, project_chat, chat_share created"
  - "prod: db_config.my_projects_enabled = '0' (closed)"
  - "prod: authenticated list_projects/create_project -> 404 'My Projects is not available on this deployment'"
  - "prod: /api/health status ok, engine vllm ready, gpu true"
  - "prod: global_search -> 200 with real corpus hits (FR-040 unchanged)"
---

## Prompt

deploy it with the gate closed

## Response snapshot

Deployed `bbcd2970` to production at 21:18 PKT with `my_projects_enabled = '0'`.

**Pre-flight.** The documented divergence trap (fired three times historically)
did **not** apply this time: `f1026525` is a strict ancestor of the branch tip and
nothing on `release` was missing locally, so this was a clean fast-forward rather
than the usual cherry-pick. Confirmed the gate is closed two independent ways
before shipping — migration 023 seeds `'0'` with `ON CONFLICT DO NOTHING`, and
`my_projects_enabled()` reads a **missing** row as off, so there is no ordering in
which the feature comes up open.

**Route.** Local pushes to GitHub time out over the VPN, so: bundle
`^f1026525 027-my-projects` → scp → fetch into the prod repo as `f027-deploy` →
`git push origin f027-deploy:release`. Checked `sudo -n -l` first: only
`systemctl restart noviq-server` is NOPASSWD, so rather than handle the sudo
password I let the **2-minute poller** do the deploy. That is safe here because
`deploy.sh` resets HEAD *before* building — a failed build cannot loop, it just
leaves the old binary running.

Frontend built in 8 s; the incremental CUDA backend took ~5 min; server restarted
and migration 023 applied on startup.

**⚠️ Two ops traps fired, both already documented and both worth re-reading.**
1. `ss -tlnp | grep postgres | head -1` selected the **wrong** listener (40247, a
   leftover test engine) and every query failed with `password authentication
   failed` — which reads exactly like a credentials problem and is not one. Prod
   must be resolved **by data dir**: `sed -n 4p .../pg/data/postmaster.pid` →
   39779.
2. `/opt/noviq/data/pg/.pgpass` holds a **bare password on one line**, not a
   5-field pgpass record, so the recorded `cut -d: -f5` recipe silently yields an
   empty string. Use `cat`.

**Verification.** The check that counts is the **authenticated** one: a signed-in
user calling `list_projects` and `create_project` gets `404 "My Projects is not
available on this deployment"`. An unauthenticated 401 would only have proved the
route exists. FR-040 confirmed live — `global_search` still returns real corpus
hits with page-level citations. (First attempt counted zero results because the
grep used the wrong JSON key; the body has `source_id`, not `id`.)

## Outcome

- ✅ Impact: F027 is in production and **dark**. Migration 023 has applied, which
  makes this a one-way step — any rollback past `bbcd2970` now requires deleting
  the migration row first.
- 🧪 Tests: six live prod checks, all green (see `tests:` above). No code changed
  in this step, so the VM suite result from PHR 0007 still stands: lib 933/2,
  `api_projects` 6/6, migration 023 2/2.
- 📁 Files: `CHANGELOG_DEV.md` status corrected from "NOT deployed" plus a deploy
  verification table; `tasks.md` T100 re-scoped from "blocked on a deploy" to
  "waiting on an admin enabling it".
- 🔁 Next prompts: enable the gate (Admin → My Projects, runtime, no restart),
  then walk the eight-step smoke test; reclaim `/opt/noviq/f027-target` (4.1 G)
  and `/opt/noviq/f027-wt` (562 M) when disk is wanted.
- 🧠 Reflection: the deploy itself was uneventful; both delays came from ops
  recipes in memory that were subtly wrong (`head -1` on two listeners, `cut -f5`
  on a one-field file). Memory corrected in place so the next deploy does not pay
  it again.

## Evaluation notes (flywheel)

- Failure modes observed: a wrong-listener connection error that mimics bad
  credentials; a verification grep whose zero result mimicked a broken feature.
  Both were resolved by looking at the actual artifact (the postmaster pid file,
  the response body) rather than trusting the summarising command.
- Graders run and results (PASS/FAIL): none — structural feature, no eval set.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): enable the gate for a single session,
  run smoke steps 1–4 (isolation only), then close it again — FR-039 says
  everything survives the round trip, and that is cheap to falsify.
