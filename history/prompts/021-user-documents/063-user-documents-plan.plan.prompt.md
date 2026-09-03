---
id: 063
title: User documents implementation plan
stage: plan
date: 2026-07-05
surface: agent
model: claude-fable-5
feature: 021-user-documents
branch: 021-user-documents
user: Riaz Mehmood
command: /sp.plan
labels: [plan, privacy, retrieval-scoping, uploads, sharing, attachments, migration]
links:
  spec: specs/021-user-documents/spec.md
  ticket: none
  adr: none
  pr: none
files:
- specs/021-user-documents/plan.md
- specs/021-user-documents/research.md
- specs/021-user-documents/data-model.md
- specs/021-user-documents/contracts/api.md
- specs/021-user-documents/quickstart.md
- CLAUDE.md
tests:
- constitution gate (13 principles) PASS with inherited §IV divergence documented
---

## Prompt

yes run the plan

## Response snapshot

Phase 0+1 complete for 021-user-documents.

Key decisions (research.md D1–D7):
- D1: scope truth on `sources` (owner_id, visibility, conversation_id) — every retrieval
  query already joins sources; folders keep visibility only as UI/default.
- D2: single `visible_source_ids(pool, RequestScope)` function; run_turn swaps
  `all_indexed_source_ids` for it; set flows through EXISTING scope_source_ids params →
  zero retrieval-SQL changes; desktop = RequestScope::unrestricted().
- D3: migration 009 backfill (owned folders → private, global → shared), no re-index.
- D4: attachments = sources with conversation_id, stub notebook `__conv__<id>`, promotion
  clears conversation_id + re-links (no re-index); delete_conversation endpoint cleans up.
- D5: allowance via env (NOVIQ_USER_QUOTA_MB=500 / _FILES=200), usage from
  SUM(file_size_bytes) on sources.
- D6: §IV PG divergence inherited (F011/F012), additive PG migration 009 + SQLite mirror.
- D7: web answer surface audit — only POST /query today; capability paths inside run_turn
  must all consume the per-turn set (dedicated task).

Constitution gate: PASS (12 clean, §IV inherited divergence in Complexity Tracking).
Contracts: /set_visibility, /attach_document, /list_conversation_attachments,
/promote_attachment, /delete_conversation, /my_usage, /admin/usage, /delete_my_document;
/query + /upload_documents changed behaviourally only. quickstart.md = SC-001..006 manual
walkthrough incl. psql invariant checks. Agent context updated.

## Outcome

- ✅ Impact: full design ready for /sp.tasks; privacy invariant isolated in one function.
- 🧪 Tests: gate re-check post-design PASS; SC test surfaces mapped in quickstart.
- 📁 Files: plan.md, research.md, data-model.md, contracts/api.md, quickstart.md.
- 🔁 Next prompts: /sp.tasks; ADR suggestion surfaced for ownership/visibility model.
- 🧠 Reflection: the existing scope_source_ids parameter is the load-bearing seam — it
  makes the privacy fix a data-flow change, not a query rewrite.

## Evaluation notes (flywheel)

- Failure modes observed: none; recon confirmed file_size_bytes exists and no web search
  endpoint exists (FR-001 surface = /query only).
- Graders run and results (PASS/FAIL): constitution 13-principle gate PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): during tasks, add a regression test that greps
  retrieval entry points for RequestScope consumption to keep the invariant honest.
