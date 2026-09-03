---
id: "0006"
title: "fix I1 C1 U1 polish"
stage: spec
date: 2026-06-02
surface: agent
model: claude-opus-4-7
feature: 007-legal-rag-pakistan
branch: 007-legal-rag-pakistan
user: Riaz Mehmood
command: "post /sp.analyze remediation patches"
labels: [feature-007, legal-rag, pakistan, analyze-remediation]
links:
  spec: specs/007-legal-rag-pakistan/spec.md
  ticket: ""
  adr: ""
  pr: ""
files:
  - path: specs/007-legal-rag-pakistan/quickstart.md
    change: modified
  - path: specs/007-legal-rag-pakistan/spec.md
    change: modified
  - path: specs/007-legal-rag-pakistan/contracts/tauri-commands.md
    change: modified
  - path: specs/007-legal-rag-pakistan/tasks.md
    change: modified
tests:
  - name: post-edit sanity scan
    status: PASS
---

## Prompt

User reply to `/sp.analyze` report:

> fix I1 and C1, for U1 we have option in setting so i we click that option we use cloud api if not selected it routes queries via local model.

This narrows U1 to a clarification rather than a frontend-add: the toggle is a persistent application Setting (not a per-query chip in Chat). FR-029 needs rewording accordingly.

## Response snapshot

Applied three targeted edits:

**I1 — quickstart.md Story 2 timing.**
Old: "Expected runtime for ~50 pages on a CPU workstation: 30–60 minutes (slow, but officer-initiated)."
New: "Expected runtime for ~50 pages on a CPU workstation: ≈ 2–4 hours" with the reasoning chain inline (research §R5 Section-boundary chunking → ~20–40 IR chunks per 50-pg Act × ~4–6 min/chunk on Mistral 7B CPU per 006 memory). Faster on GPU; resumable via `progress_store`.

**C1 — tasks.md T089 final acceptance gate.**
Added gate 6: **Network-isolation gate (SC-007)**. Harness runs in a sandbox with outbound network blocked (Linux: `unshare -rn` wrapper OR an in-process `socket2` interceptor that fails any TCP/UDP connect). Across gates 1–5 the test asserts ZERO outbound attempts. Cloud-tier code paths MUST be reachable only when `use_cloud == true` and are explicitly NOT exercised by this harness.

**U1 (revised) — Settings-toggle model.**
- spec.md FR-029 rewritten: persistent application Settings toggle, not a chat-surface chip; default off → offline; on → cloud; toggling MUST be explicit, cloud usage MUST never happen silently.
- spec.md Clarifications session 2026-06-02 — appended one new Q→A capturing the decision.
- contracts/tauri-commands.md — `ChatQueryInput.use_cloud` description updated to "resolved from persistent Settings toggle (default off). Backend MUST honour false silently and MUST route to cloud only when true. Never set true by anything other than the Settings toggle."
- tasks.md — appended new task **T096 [P]** in Phase 6: Settings UI row + persistent store + Chat.tsx wire-up + integration test asserting toggle round-trips into `LlmTier::CloudEval` only when on. NO per-query chip in Chat.tsx.
- tasks.md Task-count summary: Phase 6 7→8, total 95→96. Parallel opportunities: 38→39. Critical path: ≈ 60 → ≈ 61.

No spec FR was deleted; FR-029 was tightened from "MAY per-query toggle" to "MUST persistent Settings toggle". This strengthens compliance with constitution §I.

## Outcome

- ✅ Impact: 3 MEDIUM findings from `/sp.analyze` resolved; 0 CRITICAL / HIGH issues remain across spec/plan/tasks.
- 🧪 Tests: post-edit sanity scan PASS — no broken links, FR-029 wording change does not invalidate any existing task mapping, T096 cleanly inserts at end of Phase 6.
- 📁 Files: quickstart.md, spec.md, contracts/tauri-commands.md, tasks.md.
- 🔁 Next prompts: `/sp.implement` to start executing T001 onward (MVP scope = Phases 1+2+3, T001–T042); or `/sp.adr legal-pivot-two-tier-indexing` to record the architectural decision before implementation.
- 🧠 Reflection: User's redirection on U1 (Settings vs. per-query chip) was a tightening of constraints, not a relaxation — exactly the kind of clarification SDD's analyze step is designed to surface.

## Evaluation notes (flywheel)

- Failure modes observed: none.
- Graders run and results (PASS/FAIL): post-edit sanity scan PASS.
- Prompt variant (if applicable): n/a.
- Next experiment (smallest change to try): consider adding a constitution §XIV "Cloud-LLM opt-in is a persistent Setting, never per-query" amendment if FR-029 keeps drifting in future features — but only if drift actually appears; premature.
