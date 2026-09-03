---
id: 0002
title: My Projects implementation plan
stage: plan
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: /sp.plan
labels: [my-projects, retrieval-scoping, copy-on-share, migration-023, plan]
links:
  spec: specs/027-my-projects/spec.md
  ticket: none
  adr: proposed — copy-on-share and cross-user content linking (supersedes the deferral recorded in migration 022)
  pr: none
files:
  - specs/027-my-projects/plan.md
  - specs/027-my-projects/research.md
  - specs/027-my-projects/data-model.md
  - specs/027-my-projects/contracts/api.md
  - specs/027-my-projects/quickstart.md
tests:
  - none (planning stage; no code changed)
---

## Prompt

/sp.plan

(No arguments. Run against `specs/027-my-projects/spec.md` on branch `027-my-projects`.)

## Response snapshot

Ran the plan workflow end to end. Phase 0 research resolved every structural unknown by
reading the code rather than recalling it; Phase 1 produced the data model, contracts and
quickstart. No NEEDS CLARIFICATION markers remain anywhere.

### Phase 0 — the findings that shaped the design

**R1. `visible_source_ids` gains an exclusive mode, not a fourth branch.** Confirmed six
call sites funnel through `db/repositories/sources.rs:97`: `api/query.rs:116` (chat),
`api/documents.rs:569` (knowledge graph), `:672` (citation → file), `:754` (citation →
passage), `:864` (global search), `commands/assistant.rs:62` (desktop). Because all six
share it, FR-011 is satisfied by changing one function. A fourth OR'd branch cannot
express "and nothing else" — every existing branch only ADDS, and this is the first
requirement that must SUBTRACT.

**R2. The chat path is unwidenable by construction, and that fell out of the missing
conversation table.** There is no conversation row anywhere — `query_sessions` /
`query_messages` have zero references under `api/`, and `conversation_id` is a
client-minted string on `query_log` with no owner. Rather than treat that as a gap, the
design uses it: a new `project_chat(conversation_id PK, project_id, owner_id)` means the
client sends only `conversation_id` on a turn and **the server derives the project**. A
forged id finds no row or another user's row, which the owner check rejects. Putting
`project_id` on `query_log` was rejected precisely because `query_log` is per-turn, so
the client would re-assert the project every turn — the thing FR-013 forbids.

The four non-chat routes get no such protection: they must take `project_id` from the
client, so each calls `owned_project()` first. That asymmetry is now an explicit risk with
a test-per-route requirement, since a forgotten route fails open.

**R4. Migration 022 deferred exactly this feature's mechanism.**
`022_source_links.sql:20-24` reads: cross-user linking "would make one user inherit
another's extraction and leak the fact that the file was already in the system, and that
needs a spec + ADR of its own." This is that spec. The deferred hazard was *implicit*
dedup; copy-on-share is *explicit*, so nothing is disclosed the sharer did not choose —
but the decision is superseded on the record, not stepped over quietly.

Two consequences fell out of it, both load-bearing:

- **Membership, not ownership, is the authority.** A copy references user A's `sources`
  row, so `visible_source_ids`' `owner_id = $1` branch never matches user B. Exclusive
  mode must therefore ignore `owner_id` entirely — which also means `project_source` must
  NOT carry an `owner_id` column, unlike `source_link`, because the only owner it could
  carry is the source's and any predicate on it would break the sharing.
- **FR-009 is satisfied for free.** `usage_for_owner` sums by `owner_id`; a copy adds
  memberships, never rows, so a recipient's allowance is untouched with no extra code.

**R5. Share links must not widen `visible_source_ids`.** If viewing a shared chat added
its sources to the viewer's visible set, the viewer could immediately global-search the
owner's whole project, because global search scopes through the same function. So a
share-link viewer is authorized **per cited chunk**, not per document — a separate,
narrower read path. This is the concrete form of the architect's "view, don't continue"
decision.

**R9. Three constitution statements no longer describe the system** (Privacy-First,
SQLite, Tauri desktop), all pre-existing and all inherited without deepening.

### Phase 1 — design

Migration **023** (`project`, `project_source`, `project_chat`, `chat_share`), additive,
gate seeded **off** so applying it exposes nothing. `copied_from` is deliberately not a
foreign key — an FK with cascade would destroy the recipient's project when the sharer
deletes theirs, the exact failure FR-024 forbids.

The most dangerous change is recorded as its own rule: a document's content is now
referenced from three tables (`notebook_sources`, `source_link`, `project_source`) and the
third can belong to another user. One `is_referenced()` helper guards every delete path.
**Before 023, "delete my document" could only destroy the deleter's own data; after it, it
can destroy someone else's** — which is why the plan sequences delete-safety (phase 3)
*before* sharing (phase 4).

Constitution Check: **PASS**, with two pre-existing ⚠️ rows carried into Complexity
Tracking (Principle I via ADR-0025, Principle IV via F011) plus the 022 supersession.
Principle XI moved from "satisfied" to "strengthened": FR-016 requires every citation in a
mixed answer to declare law vs. the user's own draft, which is the failure that principle
exists to prevent.

No new crates, no new store, no new model call, no new outbound path.

## Outcome

- ✅ Impact: F027 fully planned. Five artifacts written. Three code-level findings
  materially changed the design from what the spec's notes assumed.
  ⚠️ `update-agent-context.sh claude` reported success but left `CLAUDE.md` byte-identical
  — it deduped this feature's stack against the existing F016 line
  (`Rust 1.87 (src-tauri, noviq-server bin)`), so no 027 entry exists. Verified with
  `git diff --stat`; the script's "Added language/framework/database" output is not
  evidence of a write.
- 🧪 Tests: None run — planning stage, no code changed. The quickstart names the four
  tests that matter and the per-route requirement for the isolation test.
- 📁 Files: `specs/027-my-projects/{plan,research,data-model,quickstart}.md`,
  `specs/027-my-projects/contracts/api.md`
- 🔁 Next prompts: `/sp.adr copy-on-share and cross-user content linking` (recommended
  first — migration 022 explicitly asked for it), then `/sp.tasks`.
- 🧠 Reflection: The missing conversation table looked like a gap and turned out to be the
  security property. Because there is no row for the client to name, the chat path cannot
  be widened at all — the strongest guarantee in the feature came from the thing that
  first read as an obstacle. Worth remembering before "fixing" a missing abstraction.

## Evaluation notes (flywheel)

- Failure modes observed: none. The avoided one was designing against the design notes'
  assumptions instead of the code — `notebooks` looked reusable and is not, and the
  conversation table that three candidate names implied does not exist.
- Graders run and results (PASS/FAIL): N/A — planning stage.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): implement phase 1 alone (migration 023 +
  exclusive mode + `owned_project`) with only the SC-001 negative test, run it against all
  four routes, and confirm it fails on any route where the ownership check is omitted
  before building anything on top.
