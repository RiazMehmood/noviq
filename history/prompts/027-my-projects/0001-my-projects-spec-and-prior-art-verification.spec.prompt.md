---
id: 0001
title: My Projects spec and prior-art verification
stage: spec
date: 2026-08-05
surface: agent
model: claude-opus-5
feature: 027-my-projects
branch: 027-my-projects
user: Riaz Mehmood
command: /sp.specify
labels: [my-projects, retrieval-scoping, privacy, admin-gate, spec]
links:
  spec: specs/027-my-projects/spec.md
  ticket: none
  adr: history/adr/ADR-0025 (data boundary — relevant to Open Question 1)
  pr: none
files:
  - specs/027-my-projects/spec.md
  - specs/027-my-projects/checklists/requirements.md
tests:
  - none (specification stage; no code changed)
---

## Prompt

recall memory what is remaining and start working on it.

(Which resolved, via the memory index, to the "My Projects" feature — 5 decisions
locked 2026-08-05, spec not yet written, nothing built — and then to:)

/sp.specify My Projects — user-owned document workspaces. A named, user-created
container holding its own documents and many chats, where retrieval is scoped to that
project's documents only. Starts empty (none of the 18 shared corpus documents), with an
opt-in toggle to also search the shared legal corpus. Admin gate is global on/off via
db_config, backend-enforced. Disabling hides and makes unreachable but retains data.

## Response snapshot

Verified the three open prior-art questions against the codebase **before** writing the
spec, since a stored memory flagged all three as unconfirmed. Two of the three answers
differ from what the memory assumed:

1. **`notebooks` / `notebook_sources` are LIVE, not dead F001 leftovers.** The memory
   recorded them as "unverified whether live or dead" with membership shape
   `(notebook_id, source_id, added_at)` looking ideal for reuse. In fact they are the
   active indexing-grouping mechanism: `api/uploads.rs:599` mints a stub notebook per
   conversation, `api/documents.rs:298` one per folder (`__folder__<id>`), and every
   delete path (`documents.rs:400`, `:618`, `:1232`, `:1300`) unlinks them. The table is
   already overloaded with two distinct meanings; a project is a third and must not be
   piled onto it. **Verdict: sibling table, not reuse.**

2. **No conversation table exists at all.** The memory asked which of `query_sessions`,
   `chat_message` or `query_messages` carries conversation identity, to hang `project_id`
   on. Answer: none of them. `query_sessions` / `query_messages` have zero references
   anywhere under `src-tauri/src/api/` — they are the vestigial desktop path. The live
   web identity is `query_log(user_id, conversation_id, …)` (migration 007, extended by
   020), where `conversation_id` is a **nullable, client-minted string with no owning
   row**. So there is nothing to hang a foreign key on; the project side must own the
   mapping.

3. **`visible_source_ids` is confirmed purely additive** —
   `db/repositories/sources.rs:106-121`, three OR'd branches (shared corpus / own docs /
   this conversation's attachments), each of which only ever ADDS. `RequestScope`
   (`sources.rs:53-57`) carries only `user_id` and `conversation_id`. Project scoping is
   the first requirement that must SUBTRACT, so it needs a new exclusive mode rather than
   a fourth branch.

Feature number: **027**. 026 is taken by the shipped stop-button/checkpoint work (no
spec directory, but numbered in the record), and 025 is the highest directory under
`specs/`.

Wrote `specs/027-my-projects/spec.md`: 5 prioritised user stories (P1 project-only
retrieval, P1 many chats per project, P2 corpus opt-in, P2 add-once-use-in-many, P3 admin
gate), 10 edge cases, 26 functional requirements in 6 groups, 4 key entities, 9 measurable
success criteria, plus Assumptions / Dependencies / Out of Scope and 3 Open Questions.

Encoded all 5 locked architect decisions as requirements rather than re-litigating them.
Carried the memory's four flagged risks into the spec as testable requirements: FR-011
(one scoping decision governs chat, search and citation opening alike), FR-013 (server
decides scope, client cannot widen it), FR-016 (mixed-provenance citations must be
labelled), FR-038 (hiding the entry point is explicitly not the enforcement).

**Then five further decisions were taken in-session, and the scope grew:**

- **Privacy (Q1) → disclose, don't change behaviour.** FR-041. Project documents are
  handled exactly as uploads are today and the interface says so where documents are
  added. The residual risk is recorded as Risk 4 rather than hidden, because the whole
  weight now rests on that wording.
- **Corpus toggle (Q3) → per question, defaulting to excluded.** FR-014/017/018. The
  architect asked what the option meant before choosing; explaining it as a concrete
  20-question scenario (18 about their own file, 2 needing law) got a decision
  immediately where the abstract framing had not.
- **Sharing was added to scope mid-session**, for both projects and chats.
- **Project sharing → COPY, not shared access.** FR-020/021. The recipient gets their own
  project with the same documents and becomes its owner; the original is untouched in
  both directions, forever. This is materially *simpler* than the shared-access model
  offered — no permission levels, no invitations, no concurrent editing — and it rides
  directly on the index-once/link-many mechanism Story 4 already required.
- **Chat sharing → link to view, fork to continue.** FR-026–031. Signed-in Noviq users
  open a link, read the transcript, open citations; to go further they take their own
  copy. Never reachable outside Noviq.

The architect's chat-sharing answers initially conflicted — "full access, they can
continue the chat" and then "can view the chat only". Asked rather than guessed. Guessing
either way would have produced a materially different feature, since "continue" makes
chat-sharing a back door into project access.

**Two consequences of copy-on-share were not in the answers and were added as
requirements rather than left implicit:**

- **FR-024 — a shared document cannot be recalled.** This follows necessarily from FR-007
  (content survives while any project holds it) now extending ACROSS USERS for the first
  time: the original owner deleting a document does not remove it from anyone's copy.
  Correct for a copy model, and the opposite of what users will expect, so disclosure at
  the moment of sharing is required.
- **FR-009 — copies must not be charged against the recipient's storage allowance**, since
  copying stores no additional bytes. Left unstated, an implementation would plausibly
  charge them and make copying fail for no reason the user could understand.

Final shape: 7 user stories, 18 edge cases, 41 functional requirements in 9 groups, 5 key
entities, 12 success criteria, 10 resolved decisions, 5 named risks. No
[NEEDS CLARIFICATION] markers remain; the quality checklist passes on every item.

## Outcome

- ✅ Impact: Feature 027 specified and branch created, with all open questions closed and
  sharing added to scope. Two memory assumptions corrected before they could become a
  wrong migration — `notebooks` is live and must not be repurposed, and there is no
  conversation row to attach `project_id` to.
- 🧪 Tests: None — specification stage, no code changed. The spec names the negative tests
  that must exist: SC-001 (zero shared-corpus, other-project and cross-user citations in
  a project question, across chat, global search and citation opening), SC-005 (zero
  changes leak between an original and its copy), SC-006/007 (share links refuse the
  signed-out, and refuse new questions).
- 📁 Files: `specs/027-my-projects/spec.md`,
  `specs/027-my-projects/checklists/requirements.md`
- 🔁 Next prompts: `/sp.plan`. `/sp.clarify` is no longer needed — all three open
  questions were resolved in-session. `/sp.adr` is worth considering for the copy-on-share
  model, which is architecturally significant and irreversible in the data it leaves
  behind.
- 🧠 Reflection: The memory said "check liveness first" and it was right to — the
  cheapest-looking reuse (`notebook_sources` already has exactly the right columns) was
  the wrong one, and only reading the call sites showed why. A table having the right
  shape says nothing about whether it is free. Separately: two of the architect's answers
  contradicted each other, and asking one short question rather than picking a reading
  changed the feature's security shape.

## Evaluation notes (flywheel)

- Failure modes observed: none in this session. The near-miss was reasoning from a
  schema's column list rather than from its call sites — `notebook_sources` looks unused
  in the baseline migration and is heavily used in the API layer.
- Graders run and results (PASS/FAIL): N/A — specification stage.
- Prompt variant (if applicable): N/A
- Next experiment (smallest change to try): Before `/sp.plan`, confirm how a project
  chat's identity will be recorded, given `query_log.conversation_id` has no owning row —
  this is the one structural unknown the spec deliberately leaves to planning.
