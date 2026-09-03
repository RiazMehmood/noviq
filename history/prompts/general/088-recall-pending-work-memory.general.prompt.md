---
id: 088
title: Recall pending work from memory
stage: general
date: 2026-07-16
surface: agent
model: claude-opus-4-8
feature: none
branch: 025-agentic-deep-search
user: Riaz Mehmood
command: "recall memory what is pending still?"
labels: [memory, backlog, recall]
links:
  spec: none
  ticket: none
  adr: none
  pr: none
files: []
tests: []
---

## Prompt

recall memory what is pending still?

## Response snapshot

Read-only recall over the memory index (`MEMORY.md`) plus `project_context-window-budget.md`.
No code changed. Reported the open items grouped by priority:

**Top of stack (next actions)**
- Re-land the two reverted conversation-memory commits ONE AT A TIME, watching the newly traced
  prompt overhead: `325618ab` (4-pair history window) and `68d4f71a` (pinned facts + summary
  injection). Both were reverted in `80048b37` for context overflow; the budget fix `9f40d4aa`
  now absorbs them by trimming law instead of overflowing. Strip any literal `[n]` from new
  prompt text (that substring flips the citation demo on).
- Deep-search checkpoint RESUME has never been exercised live — needs the user to set
  `ECHO_DEEP_CHECKPOINT=1` and restart. Resume is also suspected dead code (per-turn thread_id).
- Deep search open defects blocking public rollout: synthesis garble tail, appeal-confusion /
  instrument-scoping, enumeration detection too narrow ("list down").

**Data / indexing**
- Constitution, Compendium and Manual have 0 `document_full` rows → backfill via
  Admin → Indexing → Build summaries.
- RoB Schedule-I fragmentation: parse-time table chunking; granularity is an architect decision.
- Multi-page table stitching (departments count 32 ≠ 39).
- Doc-depth reindexing: `manual.rs` port for RoB; reclassify SPPA-2009 manual→act; act.rs+Item
  for constitution; SLGA only 118 chunks.
- Constitution Art 55-76 still missing (chunking audit); E&D-1973 re-ingest.

**Docs / artifacts debt**
- ADR + PHRs for the GraphRAG own-build; VM worktree cleanup.
- ADR/PHR for document-full summaries; decide `ECHO_SUMMARY_RECALL=on` in prod.
- ADR for F022 Notice Assist; TOML persona fragments (Principle VI).
- CHANGELOG/PHR for the summary-index build + functional eval.

**Parked / backlog**
- Web build leaks desktop-only Tauri code (`stores/scheduler.ts:34` top-level `listen`,
  `api/assistant.ts`) — user parked, nothing user-visible broken.
- Chat token streaming over the tunnel needs WS (SSE is buffered to nothing).
- Attachment content-dedup: needs spec + ADR; cross-user linking is the risky part.
- "Copy draft" still plain-text only.
- F018 deferred: desktop cmd, admin FE panel, re-hydrate.
- Phase-2b GLiNER + Phase-3 polish for confidential masking.
- RAG-SFT training not started; reranker hardcoded OFF (mis-ranked the legal corpus).
- Phase-3 smart-widen-guard for document-scope routing.
- Rollout deep search to all users = `ECHO_DEEP_AUDIENCE=all` + restart (no rebuild) once the
  defects above are smoothed.

## Outcome

- ✅ Impact: Consolidated backlog surfaced from memory; no code or config changed.
- 🧪 Tests: none (read-only recall).
- 📁 Files: none modified.
- 🔁 Next prompts: re-land `325618ab`, then `68d4f71a`, verifying traced overhead after each.
- 🧠 Reflection: The memory index carries the backlog well, but "pending" is spread across ~15
  entries; a single rolled-up backlog note would make this recall cheaper next time.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): n/a.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): keep a single consolidated backlog memory entry.
