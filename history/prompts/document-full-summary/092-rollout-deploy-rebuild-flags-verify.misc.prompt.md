---
id: 092
title: GraphRAG rollout — deploy, rebuild overlay, flip flags, verify live
stage: misc
date: 2026-07-12
surface: agent
model: claude-opus-4-8
feature: document-full-summary
branch: feat/document-full-summary
user: Riaz Mehmood
command: (conversation)
labels: [graphrag, rollout, deploy, ops, verification]
links:
  spec:
  ticket:
  adr: history/adr/0020-graph-augmented-retrieval-on-postgres-not-neo4j.md
  pr:
files:
  - /opt/noviq/echoAI/.env (prod, ECHO_GRAPH_EXPAND=1 + ECHO_AMEND_NOTES=1)
tests:
  - live prod query trace 515b824f (hybrid+graph + graph-expansion + amendment-annotation)
---

## Prompt

"ok go accordingly rollout 4 step" → "yes, deploy to release / production" → (resume) "start working again" → "yes, set ECHO_GRAPH_EXPAND=1 and ECHO_AMEND_NOTES=1 and restart".

## Response snapshot

Step 1 — verified `release` fast-forwards (660ef402 → ad0e80dd, no divergence), pushed `feat/document-full-summary:release` (user-confirmed prod deploy). Deploy timer (`noviq-deploy`, 2-min poll) pulled + incremental cuda build + restart; server live 17:02:32.

Step 2 — admin login, `POST /api/rebuild_graph` → `{intra_doc_edges:627, cross_doc_edges:7, instruments_targetable:7, sources:16}`. Overlay 116 (constitution-only) → corpus-wide.

Step 3 — audit (PG port drifted to 35825): 616 `cross_reference` + 18 `non_obstante` across 13 docs; cross-doc edges clean (SPPRA→SPP Act ×3, Compendium→SLGA ×2, Removal→E&D ×1, Compendium→Removal ×1) — no junk.

Step 4 — set `ECHO_GRAPH_EXPAND=1` + `ECHO_AMEND_NOTES=1` in prod `.env` (backup `.env.bak.pre-graphflags.*`), restarted noviq-server (17:08:26). Live query (`POST /api/query {text, conversation_id}` — both required strings) → trace `515b824f`: STEP "Graph expansion (legal overlay)" (8 seeds → +1 provision), STEP 13 Strategy `hybrid+graph`, STEP 15 "Amendment-awareness (Phase 2)" annotated 1 of 33. Cross-doc link surfaced (Delegation Rule 7 → Sindh Public Procurement Act 2009).

## Outcome

- ✅ Impact: Feature fully live in production and verified end-to-end.
- 🧪 Tests: live trace 515b824f confirms tier + both flag behaviours fire; overlay audit confirms edges.
- 📁 Files: prod .env (2 flags); no repo code (rollout only)
- 🔁 Next prompts: ADR-0020 + PHRs (this batch); VM worktree cleanup
- 🧠 Reflection: Flag-gated OFF meant the deploy was a no-op until the explicit flag flip — deploy and behaviour-change were cleanly separable and each got its own confirmation.

## Evaluation notes (flywheel)

- Failure modes observed: PG port drifts every restart (find via `ss -tlnp|grep postgres`); query API needs `text` + string `conversation_id` (not `query`/null).
- Graders run and results (PASS/FAIL): live prod trace PASS (hybrid+graph, graph-expansion, amendment-annotation all present)
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): formal A/B (flag on vs off) over a completeness question set for ADR evidence.
