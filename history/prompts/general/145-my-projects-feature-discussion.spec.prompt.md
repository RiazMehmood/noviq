---
id: 145
title: My Projects — user-owned document workspaces, requirements discussion
stage: spec
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: my-projects
branch: release-deploy
user: Riaz Mehmood
command: "discuss the My Projects feature"
labels: [feature, retrieval-scoping, user-documents, admin-gate, discussion]
links:
  spec: pending
  ticket: n/a
  adr: suggested — retrieval scoping gains an exclusive mode
  pr: n/a
files:
  - src-tauri/src/db/repositories/sources.rs (read — visible_source_ids, the seam)
  - src-tauri/src/db/pg/migrations/022_source_links.sql (read — link-table prior art)
  - src-tauri/src/db/pg/migrations/021_corpus_categories.sql (read — disable pattern)
  - src-tauri/src/db/pg/migrations/001_baseline.sql (read — notebooks/notebook_sources)
  - src-tauri/src/db/pg/migrations/005_server_users_sessions.sql (read — users.role)
tests:
  - "none — discussion/clarification turn, no code changed"
---

## Prompt

now our corpus is clear, update this in memory so it remains update, now we want to create a new feature named my projects, which looks like same like chat page, but it does not have any document indexed it is cleared and operated by user only so user can index his own documents and search only inside those documents and can assign name a project seperately, discuss this do u undertand it clearly?

(mid-turn addition) and one more thing this feature remains in admin control it can enable and disable it also.

## Response snapshot

Confirmed understanding and grounded it in the tree rather than speculating.

**The feature**: a named, user-created container holding its own documents and
many chats, where retrieval is scoped to that project's documents only, with an
opt-in toggle to also reach the shared legal corpus; documents indexed once and
linked into multiple projects; the whole feature admin-switchable.

**Decisions taken this turn (architect):**
1. Corpus access — **project-only, with an opt-in toggle** per project/question.
2. Structure — **container with MANY chats**, not one chat per project.
3. Reuse — **index once, link to both**, reusing migration 022's dedup.
4. Admin disable — **hidden + unreachable, data retained**, fully reversible.
5. Gate scope — **global on/off only**, like deep search; per-user later.

**The core technical finding**: `visible_source_ids`
(`db/repositories/sources.rs:97`) is purely ADDITIVE — shared corpus OR own docs
OR this conversation's attachments. Every branch only adds. Project scoping is
the first requirement that must SUBTRACT, so the function needs a new exclusive
mode. Because global search and citation opening scope through that same
function, project isolation is inherited by them for free — the same property
that makes a 021 admin-disable take effect everywhere with no re-index.

**Prior art flagged, not assumed**: `notebooks` + `notebook_sources` exist in the
001 baseline with exactly the membership shape needed. Must check whether they
are live or dead before reusing/renaming/adding a sibling.

## Outcome

- ✅ Impact: requirements settled on 5 explicit decisions; retrieval seam and prior art identified. No code changed.
- 🧪 Tests: n/a (discussion turn).
- 📁 Files: none modified; five read for grounding.
- 🔁 Next prompts: `/sp.specify` for My Projects; verify whether `notebooks`/`notebook_sources` are live; locate where conversation identity is stored to hang `project_id` on.
- 🧠 Reflection: the answer that mattered came from reading `visible_source_ids` rather than from the feature description — "search only inside those documents" sounds like a filter, but the function had no way to express exclusion at all. Grounding a feature discussion in the actual seam changes what the work IS.

## Evaluation notes (flywheel)

- Failure modes observed: none this turn.
- Graders run and results (PASS/FAIL): n/a
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): before specifying, confirm the `notebooks` tables' liveness and the conversation-identity table — both change the schema shape materially.
