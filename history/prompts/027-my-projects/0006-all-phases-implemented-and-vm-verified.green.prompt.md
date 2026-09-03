---
id: 0006
title: All phases implemented and VM verified
stage: green
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: /sp.implement (continued — "complete all phases then check once on VM")
labels: [my-projects, migration-023, retrieval-scoping, vm-verified, serde-fails-open]
links:
  spec: specs/027-my-projects/spec.md
  ticket: none
  adr: history/adr/0028-copy-on-share-and-cross-user-content-linking.md, history/adr/0029-project-retrieval-isolation-via-a-subtractive-scoping-seam.md
  pr: none
files:
  - src-tauri/src/api/projects.rs
  - src-tauri/src/api/documents.rs
  - src-tauri/src/api/query.rs
  - src-tauri/src/api/uploads.rs
  - src-tauri/src/api/system.rs
  - src-tauri/src/api/router.rs
  - src-tauri/src/db/repositories/projects.rs
  - src-tauri/src/db/repositories/chat_shares.rs
  - src-tauri/src/db/repositories/sources.rs
  - src-tauri/src/services/projects/mod.rs
  - src-tauri/src/services/assistant/mod.rs
  - src-tauri/tests/api_projects.rs
  - src-tauri/tests/migration_023_test.rs
  - src/pages/ProjectsPage.tsx
  - src/pages/ProjectPage.tsx
  - src/pages/SharedChatPage.tsx
  - src/components/CorpusToggle.tsx
  - src/stores/projects.ts
tests:
  - "cargo check --lib (VM): 0 errors"
  - "cargo test --lib (VM): 933 passed / 2 failed — baseline 925/2, delta is exactly the 8 new tests"
  - "migration_023_test (VM, real PG): 2/2 pass"
  - "api_projects --ignored (VM, real PG + router): 5/5 pass"
  - "npx tsc --noEmit: exit 0"
  - "Sabotage check: exclusive branch disabled → isolation test FAILS with 'SHARED CORPUS leaked into a project'"
---

## Prompt

continue with Phase 3

…then, mid-turn: "complete the implementations of all phases then check one time
only on vm." …then: "which two test are getting in failed state? and why? and
where do they belong?" …then: "save progress upto now as context grows too much
we need start fresh and run the remaining tests."

## Response snapshot

Implemented Phases 3–9 (US1–US7) and verified the whole feature on the VM.
**95 of 101 tasks complete**; 6 polish tasks remain. Four commits on
`027-my-projects`, head `950a45b0`. Nothing pushed, nothing deployed, gate seeds
to `'0'`.

### VM verification — real numbers, not assumptions

Detached worktree `/opt/noviq/f027-wt` on temp ref `refs/f027-check` with its own
`CARGO_TARGET_DIR`, so the deploy poller stayed inert throughout.

| check | result |
|---|---|
| `cargo check --lib` | **0 errors** — ~2000 lines of Rust written with no local toolchain |
| `cargo test --lib` | **933 passed / 2 failed** (baseline 925/2; delta = exactly the 8 new tests) |
| `migration_023_test` | **2/2** against real PostgreSQL |
| `api_projects --ignored` | **5/5** against the real axum router + PG |
| `tsc --noEmit` | exit 0 |

⭐ **The isolation test was sabotage-checked.** Disabling the exclusive branch
(`if false {`) made `project_scope_excludes_everything_but_the_project` fail with
`SHARED CORPUS leaked into a project`, then restoring it made it pass. A negative
test that passes without the code it guards is testing nothing, and this is the
cheapest moment to find that out.

### ⚠️⚠️ The finding that matters most: camelCase + Option fails OPEN

`api_projects::project_scope_isolates_every_route` failed asserting that bob
scoping to alice's project is refused — it got **200, not 404**.

Cause: `GlobalSearchReq`, `SourceChunksReq` and `KnowledgeGraphReq` all carry
`#[serde(rename_all = "camelCase")]`, so the wire key is `projectId`, while
`contracts/api.md` documents `project_id`. The mismatch fails **silently and in
the worst possible direction**: serde yields `None`, `scope_for` takes the
unscoped branch, and the route answers 200 with the caller's entire visible
corpus instead of refusing. Fixed with `#[serde(default, alias = "project_id")]`,
matching the alias pattern `QueryReq` already uses for `conversationId`.

⭐ **It also meant the POSITIVE isolation assertions in that same test were
passing vacuously** — an unscoped search over sources with no chunks returns no
hits, so nothing appeared to leak. Only the negative "bob must be refused"
assertion caught it. This is a concrete argument for writing the refusal
assertion, not just the "nothing leaked" one.

Second, smaller trap: `source_chunks` has always taken `sourceId` on the wire, so
the test's `source_id` produced a 422 rather than reaching the ownership check —
a test bug, fixed in the test.

### Environment problems solved along the way

- Non-login SSH has no `cargo` on PATH → `export PATH=$HOME/.cargo/bin:$PATH`.
- The `api_admin`-style harness (`init_db` into a temp dir) is **unrunnable on
  this VM**: `initdb: error while loading shared libraries: libssl.so.3`. Switched
  to `db::testing::test_pool()`, which shares one engine honouring
  `NOVIQ_TEST_PG_INSTALL_DIR`. That is why `api_admin` is `#[ignore]`d and, in
  practice, has probably not run here in a long time.
- Anything touching `global_search` needs the ONNX embedder:
  `ORT_DYLIB_PATH=/opt/noviq/.ort-runtime/libonnxruntime.so.1.20.1` plus
  `LD_LIBRARY_PATH=/opt/noviq/.ort-runtime`.
- The VM repo is `/opt/noviq/echoAI` (not `/opt/noviq/app`) on `release` at
  `f1026525`; a bundle must be cut from `f1026525..HEAD` or the fetch fails on
  missing prerequisite commits.

### The two pre-existing failures (asked about directly)

Both were failing before this work and are unrelated to it:

1. `sources::scope_tests::conversation_scope_includes_own_attachment` — in the
   very module this feature changes, which is why quickstart.md warns about it.
   It is a **stale fixture**, not a live defect: it seeds
   `sources.conversation_id` but never inserts a `source_link` row, so it encodes
   pre-022 behaviour. Evidence it is independent of this work: it failed
   identically in both the working and the deliberately-sabotaged build.
   ⚠️ Worth a ticket — it is the only coverage of the attachment branch's owner
   predicate, so that guard is currently untested. Fix by adding the link row;
   do not delete the test.
2. `services::structured_importer::tests::rcb_parliament_articles_with_bracket_openers_all_survive`
   — the parser drops Article 59 on RC-B bracket openers. Different subsystem
   entirely; F027 touches no parsing.

## Outcome

- ✅ Impact: F027 implemented end to end and verified on real infrastructure.
  Production is untouched and safe: nothing deployed, and the gate seeds closed.
- 🧪 Tests: see the `tests:` block above. One real defect found and fixed by the
  tests; one test bug found and fixed.
- 📁 Files: 18 source files + 2 test files; tasks.md at 95/101.
- 🔁 Next prompts: the 6 polish tasks — T096 `CHANGELOG_DEV.md`, T097 `CLAUDE.md`
  **by hand** (`update-agent-context.sh` reports success and writes nothing when
  the stack dedupes), T098 docs note that a shared document cannot be recalled,
  T099 perf, T100 quickstart walkthrough, T101 promote both ADRs to Accepted.
- 🧠 Reflection: the compiler validated ~2000 blind-written lines at zero errors,
  and then found nothing that mattered. Both real problems were **silent
  fallbacks** a type system cannot see: a missing serde key that degrades to
  "unscoped" rather than failing. Writing the refusal assertion — not merely the
  "nothing leaked" one — is what turned an invisible security hole into a red test.

## Evaluation notes (flywheel)

- Failure modes observed: (a) camelCase/snake_case scope field failing open —
  fixed; (b) positive isolation assertions passing vacuously against a chunk-less
  fixture — mitigated by the negative assertion, but worth remembering that a
  fixture with no chunks cannot demonstrate a leak; (c) test harness copied from
  `api_admin` was unrunnable on the VM.
- Graders run and results (PASS/FAIL): `cargo check` **PASS**; `cargo test --lib`
  **PASS** (no new failures vs baseline); `migration_023_test` **PASS**;
  `api_projects` **PASS** 5/5; `tsc` **PASS**; sabotage check **PASS** (test
  correctly fails when the guarded code is removed).
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): give the isolation fixture real
  chunks and embeddings so a scoped `global_search` returns actual hits. Today
  its positive assertions are satisfied by an empty result set, which is exactly
  how the camelCase bug hid — the negative assertion is currently carrying the
  whole test.
