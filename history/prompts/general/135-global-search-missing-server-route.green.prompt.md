---
id: 135
title: Global Search missing server route
stage: green
date: 2026-07-31
surface: agent
model: claude-opus-5
feature: general
branch: release-deploy
user: Riaz Mehmood
command: (freeform)
labels: [api, axum, search, privacy, scoping, deploy, cherry-pick]
links:
  spec: N/A
  ticket: N/A
  adr: N/A
  pr: N/A
files:
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/router.rs
  - CHANGELOG_DEV.md
tests:
  - "cargo check --no-default-features --features server (VM worktree, isolated CARGO_TARGET_DIR): PASS"
  - "post-deploy probe POST /api/global_search: 401 (was 405) - route live behind auth"
  - "post-deploy health: ok; noviq-server restarted 19:57 PKT on 4372f02"
  - "real authenticated search query: NOT RUN - no credentials on this machine"
---

## Prompt

now it is good responsive working fine on mobile also, but there is a problem in
global search it is showing api error on web and mobile both.

resolve global search bug and deploy it by cherry picking.

## Response snapshot

**Diagnosis.** Not a responsive regression — `SearchPage.tsx` had only had padding
changed. `global_search` was registered only as a Tauri command (`lib.rs:124`);
the HTTP route was never added when the product became client-server, so
`/api/global_search` answered **405** and the frontend showed "API error". Found
by probing unauthenticated: live routes answer 401, absent ones 405.

**Fix.** New `api/documents.rs::global_search` + one route line, scoped through
`visible_source_ids` (shared corpus + caller's own documents). Deliberately NOT a
verbatim port of the desktop command, which selects across every indexed chunk
and would have leaked other users' private uploads on a multi-user server — the
same lesson this repo already recorded for `browse_chunks`/`source_chunks`.
Conversation attachments excluded (corpus search, not a chat turn); embedding on
`spawn_blocking`; response shape identical to `SearchResult` so no client change.

**Deploy.** Cherry-picked onto `release` as `4372f02`, pushed via bundle-through-VM
(local GitHub pushes time out on the VPN). Backend change ⇒ `BE=1` ⇒ CUDA rebuild
+ `systemctl restart noviq-server`; ~4 min build, then healthy. Verified 401.

## Outcome

- ✅ Impact: Search tab now has a server route for the first time on the web
  build, and it cannot read documents the caller may not see.
- 🧪 Tests: `cargo check` clean in an isolated VM worktree; post-deploy probe 401;
  health ok. No authenticated end-to-end query run — that needs the architect.
- 📁 Files: 2 source, 1 changelog.
- 🔁 Next prompts: confirm a real search returns hits; consider whether hybrid
  FTS+vector (as `query_router` uses) should back this instead of pure vector.
- 🧠 Reflection: the 401-vs-405 probe turned a vague "API error" into an exact
  answer in one command. Worth reaching for before reading any frontend code.

## Evaluation notes (flywheel)

- Failure modes observed: **the branch-divergence trap fired twice.** (1) The
  responsive cherry-pick was nearly made onto a stale local `release`, which would
  have reverted two live websearch fixes — caught by comparing against the VM's
  deployed HEAD before pushing. (2) The first `cargo check` failed on
  `admin::inspect_source`/`split_source`, purely because `release-deploy`'s
  `router.rs` was copied onto a `release`-based worktree. **Compile errors from a
  cross-branch file copy are not errors in the change.** Both caught pre-push.
- Graders run and results (PASS/FAIL): cargo check PASS, route probe PASS,
  end-to-end query NOT RUN.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): always `git fetch` the VM's `release`
  and diff against deployed HEAD *before* starting any cherry-pick — both failures
  above share that one root cause.
