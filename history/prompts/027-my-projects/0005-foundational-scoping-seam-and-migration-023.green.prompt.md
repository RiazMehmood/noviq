---
id: 0005
title: Foundational scoping seam and migration 023
stage: green
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: /sp.implement
labels: [my-projects, migration-023, retrieval-scoping, unverified, needs-vm-compile]
links:
  spec: specs/027-my-projects/spec.md
  ticket: none
  adr: history/adr/0028-copy-on-share-and-cross-user-content-linking.md, history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  pr: none
files:
  - src-tauri/src/db/pg/migrations/023_my_projects.sql
  - src-tauri/src/db/repositories/sources.rs
  - src-tauri/src/db/repositories/projects.rs
  - src-tauri/src/db/repositories/chat_shares.rs
  - src-tauri/src/db/repositories/mod.rs
  - src-tauri/src/services/projects/mod.rs
  - src-tauri/src/services/mod.rs
  - src-tauri/src/api/projects.rs
  - src-tauri/src/api/mod.rs
  - src-tauri/tests/migration_023_test.rs
  - src/stores/projects.ts
  - specs/027-my-projects/tasks.md
tests:
  - "TypeScript: npx tsc --noEmit — PASS (exit 0)"
  - "Rust: NOT COMPILED, NOT RUN — no Rust toolchain on this Windows host"
  - "9 Rust tests written and unexecuted (6 scope_tests, 1 gate unit, 2 migration)"
---

## Prompt

/sp.implement

(No arguments. Run against `specs/027-my-projects/tasks.md` on branch `027-my-projects`.)

## Response snapshot

Checklist gate: `requirements.md` 16/16 complete → **PASS**, proceeded automatically.
Ignore-file verification: repo is git, `.gitignore` already carries `target/`,
`node_modules/`, `dist/`, `.env`, `.DS_Store` — no changes needed.

Implemented **Phase 1 (Setup, T001–T005)** and **Phase 2 (Foundational, T006–T017)**, plus
**T050** (`is_referenced`) pulled forward from Phase 5 because the exclusive-mode work and
the delete-safety helper touch the same file and the helper is what every later phase
depends on. **18 of 101 tasks complete.** Stopped at the checkpoint the plan, the tasks
file and the prior turn all name: `T014` must be seen to fail before `T011` and pass after,
and that cannot happen on this host.

### ⚠️ Nothing Rust was compiled or run

There is no Rust toolchain on this Windows machine (confirmed: `which cargo rustc` → not
found). Roughly 900 lines of Rust were written **and never type-checked**. The 9 tests
written — 6 in `sources.rs` `mod scope_tests`, 1 gate unit test, 2 in
`migration_023_test.rs` — have never been executed, so their pass/fail state is unknown.
This is the whole caveat of this turn and is repeated in the final report rather than
buried here.

What WAS verified:

- `npx tsc --noEmit` → **exit 0**. Caught a real defect: `src/stores/projects.ts` imported
  `call` from `../api`, but `api/index.ts` imports it privately as `invoke` and never
  re-exports it. Corrected to `../api/transport`, matching what `auth.ts` and
  `categories.ts` already do.
- Every Rust symbol used was checked against its definition by grep rather than assumed:
  `AppState` and its `pool: crate::db::Db` field (`api/mod.rs:48-49`), `ApiError::new` and
  `ApiError::internal` (`api/mod.rs:70,79`), `pub type Db = sqlx::PgPool`
  (`db/mod.rs:10`), `uuid` v4 and `chrono` in `src-tauri/Cargo.toml:107-108`, and
  `sqlx::migrate!("./src/db/pg/migrations")` (`db/schema.rs:53`) confirming a new `.sql`
  file needs no registration.

### The scoping seam (ADR-0029)

`RequestScope` gained `project_id: Option<String>` and `include_corpus: bool`;
`unrestricted()` and `Default` keep today's values, so no existing call site changes
meaning. `visible_source_ids` gained an exclusive branch **before** the existing logic,
returning early so it cannot fall through to the additive query. Two things it
deliberately does not consult, both documented in place: `owner_id` (a copied project
holds the *sharer's* rows, so any owner predicate would break sharing) and conversation
attachments.

The invariant is now written above the function: this is the only legal place to scope
retrieval, per-call-site filtering is forbidden, and why.

### The tests that gate the phase

`project_scope_excludes_everything_but_the_project` asserts each excluded category
**separately** — shared corpus, the same user's other project, another user's document,
another user's private document — so a failure names which one leaked rather than only
that something did. The fixture deliberately makes every excluded category non-empty; an
isolation test against an empty database proves nothing.

Also written: the corpus opt-in widening to the corpus and nothing further; a test that
exclusive mode **ignores `owner_id`** (userB scoping to a project holding userA's
document, simulating a copy — this is the test that would fail if someone "fixed" the
missing owner predicate); empty-project; disabled-document; and `is_referenced` seeing
each of the three reference types independently.

### Two design points implemented as written rather than as convenient

- `project_source` has **no `owner_id`**, and the migration and the schema test both say
  why at length. It is the column a future maintainer would add in good faith, and adding
  it breaks copy-on-share.
- `project.copied_from` has **no foreign key**, asserted by the migration test querying
  `information_schema` — a cascade from it would destroy a recipient's project when the
  sharer deleted theirs.

## Outcome

- ✅ Impact: The schema and the security seam exist, with the negative test that must
  prove them. Feature is inert in production: the gate seeds to `'0'`, so applying
  migration 023 exposes nothing.
- 🧪 Tests: TypeScript **PASS** (exit 0). Rust **NOT RUN** — 9 tests written, 0 executed,
  0 known to pass. No compile has happened.
- 📁 Files: 10 Rust/SQL files, 1 TypeScript, tasks.md updated to 18/101.
- 🔁 Next prompts: compile and run on the VM per `quickstart.md`
  (`NOVIQ_TEST_PG_INSTALL_DIR=... cargo test --no-default-features --features server`),
  using the detached-worktree recipe so the deploy poller stays inert. Confirm `T014`
  fails with the exclusive branch removed before trusting it. Then Phase 3 (US1).
- 🧠 Reflection: The one defect actually found this turn was found by the only checker
  available — `tsc` caught an import that reads correctly and resolves to nothing. That is
  a fair estimate of what the ~900 unchecked lines of Rust are hiding: not design errors,
  but the ordinary kind a compiler finds in seconds and a human does not find by reading.

## Evaluation notes (flywheel)

- Failure modes observed: one real defect, caught and fixed (`call` imported from a module
  that does not export it). The larger risk is unresolved rather than observed: Rust was
  written blind and its correctness is asserted by grep, not by a compiler.
- Graders run and results (PASS/FAIL): `npx tsc --noEmit` **PASS**. `cargo test` **NOT
  RUN**. Checklist gate `requirements.md` 16/16 **PASS**.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): on the VM, comment out the exclusive-mode
  branch in `visible_source_ids` and confirm `project_scope_excludes_everything_but_the_project`
  **fails**, then restore it and confirm it passes. A negative test that passes without the
  code it guards is testing nothing, and this is the cheapest moment to find that out.
