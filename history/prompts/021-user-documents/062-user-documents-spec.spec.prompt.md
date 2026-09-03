---
id: 062
title: User documents feature spec
stage: spec
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: 021-user-documents
user: Riaz Mehmood
command: /sp.specify
labels: [spec, privacy, uploads, sharing, attachments, retrieval-scoping]
links:
  spec: specs/021-user-documents/spec.md
  ticket: none
  adr: none
  pr: none
files:
- specs/021-user-documents/spec.md
- specs/021-user-documents/checklists/requirements.md
tests:
- checklist validation (all items pass)
---

## Prompt

one big spec, cover all four, write specs first

(Full feature description passed to /sp.specify): User documents with ownership-scoped
retrieval, uploads, sharing, and chat attachments — (1) retrieval respects document
ownership: chat/search only retrieves from documents the requesting user may see (their
own + shared/global corpus); (2) regular web users can upload and index their own
documents from a simple "My documents" UI; (3) owners/admins can toggle a document/folder
between private and shared-with-everyone via an explicit visibility model; (4) users can
attach a file inside a chat conversation so the LLM sees its content for that conversation
only (conversation-scoped indexing, promotable to My documents)

## Response snapshot

Created branch `021-user-documents` (next number after 020) + spec covering all four gaps
found in the earlier code audit:

- US1 (P1) ownership-scoped retrieval — foundation; today `run_turn` scopes to ALL indexed
  sources with no owner filter, so "private" is listing-only. FR-001..004: every
  answer-producing surface restricted to global + own + active-conversation attachments.
- US2 (P2) "My documents" upload UI for ordinary web users (backend endpoint already
  exists; UI is admin-gated today). Per-user allowance (default 500 MB / 200 files,
  admin-configurable), delete, retry, audit.
- US3 (P3) explicit Private/Shared visibility with owner/admin toggle; pre-existing admin
  corpus migrates to Shared with NO re-indexing (FR-015).
- US4 (P4) conversation attachments: indexed for one conversation only, cited, die with
  the conversation, promotable to My documents.

Two [NEEDS CLARIFICATION] policy questions asked and answered by user:
1. Sharing flow = IMMEDIATE (owner-decided, no admin approval queue; admin retains
   after-the-fact unshare/delete + audit trail).
2. Admin answer scope = GLOBAL + OWN ONLY (admin chat follows same privacy rule as users;
   management powers unchanged).

Quality checklist at checklists/requirements.md — all items pass.

## Outcome

- ✅ Impact: complete spec for feature 021 covering the retrieval-privacy fix + three user
  powers, with both open policy decisions resolved by the user.
- 🧪 Tests: checklist validation all-pass; acceptance scenarios defined per story.
- 📁 Files: specs/021-user-documents/spec.md, specs/021-user-documents/checklists/requirements.md.
- 🔁 Next prompts: /sp.plan (design: visibility column vs owner_id NULL, scoped retrieval
  seam in run_turn/all_indexed_source_ids, conversation-scoped notebook for attachments).
- 🧠 Reflection: the four asks compose cleanly into one feature because they share a single
  root cause (ownership missing from retrieval); P1 ordering makes privacy the MVP.

## Evaluation notes (flywheel)

- Failure modes observed: create-new-feature.sh switched branch off `release` as designed;
  spec skeleton required Read-before-Write.
- Graders run and results (PASS/FAIL): requirements checklist PASS (all items).
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): at plan time, decide whether folder-level and
  document-level visibility need separate storage or folder cascades to documents.
