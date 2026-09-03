---
id: 0002
title: plan one-ask drafting pipeline
stage: plan
date: 2026-06-11
surface: agent
model: claude-fable-5
feature: 010-one-ask-drafting
branch: 010-one-ask-drafting
user: Riaz Mehmood
command: /sp.plan
labels: [plan, drafting, vec0-knn, mistral-7b, citation-grade]
links:
  spec: specs/010-one-ask-drafting/spec.md
  ticket: null
  adr: pending — composition architecture ADR suggested post-plan
  pr: null
files:
  - specs/010-one-ask-drafting/plan.md
  - specs/010-one-ask-drafting/research.md
  - specs/010-one-ask-drafting/data-model.md
  - specs/010-one-ask-drafting/contracts/one-ask-commands.md
  - specs/010-one-ask-drafting/quickstart.md
  - CLAUDE.md (agent-context auto-update)
tests:
  - planned: one_ask_acceptance.rs (canned model, no LLM), knn_parity, lib drafting suite, tsc
---

## Prompt

Plan Feature 010 one-ask-drafting per specs/010-one-ask-drafting/spec.md. Technical context: Rust 1.87 src-tauri + SolidJS frontend, Tauri 2.0, sqlx 0.8 SQLite at echo.db, fastembed 4 + sqlite-vec (vec0) retrieval, llama-cpp-2 0.1.143. Writer model = local Mistral-7B-Instruct-v0.3 Q4_K_M at models/ir_builder/ (free-text only — llguidance grammar path crashes, proven twice). Reuse: decompose::resolve_provision_label citation resolution (hyphen-tolerant), Feature 009 verbatim-verify gate concepts (ingest.rs gate), chat_message persistence (migration 023), draft_session (migration 022), DraftSessions store, progress_store/event patterns. Must fix: retrieve_whole_corpus brute-force vec scan (281s) → vec0 KNN MATCH/k=? form. Composition design: built-in 5-section Sindh summary skeleton; per-section retrieval → verbatim provision cards; 7B writes connective prose with [CITE:n] markers only; deterministic splice + verify; officer figures injected verbatim from parsed intent; flags for unsupported content. Frontend: replace drafting tab flow with single-ask chat + draft view; remove template manager/gap panel from officer path.

## Response snapshot

Phase 0 research.md: 9 decisions — R1 vec0 KNN two-step (MATCH ?/k=? over-fetch then hydrate/filter; L2≡cosine for normalised embeddings, parity test gates it; codebase grep confirmed NO MATCH usage exists anywhere yet — every vec query brute-forces), R2 Mistral-7B free-text [CITE:n]/[FIG:n] markers (llguidance dead), R3 deterministic post-scan gate (stray provision-numbers/figures → Flag runs, never silent), R4 per-section composition over hardcoded 5-section skeleton const (n_ctx budget + targeted revision routing), R5 per-section retrieval → provision cards labelled via decompose::resolve_provision_label, R6 regex-only intent parse (Tier 1 discipline), R7 additive migration 025 one_ask_session (parallel to 009 draft_session, no serde-shape mixing), R8 spawned task + drafting://one-ask-progress events + cancel token + fail-fast on low RAM tiers, R9 frontend-only removal of 009 officer path (backend retained, style few-shot deferred). Phase 1: plan.md (13-principle constitution check PASS, XIII noted deferred-enhancement; module layout services/drafting/one_ask/{intent,skeleton,research,compose,verify,state}.rs + shared retrieval.rs + commands/one_ask.rs + OneAskView.tsx), data-model.md (OneAskState/ProvisionCard/Paragraph/Run tagged enum with 4 gate invariants; migration 025 SQL), contracts/one-ask-commands.md (start/revise/cancel/get/retry/finalize + 2 events + contract acceptance checks), quickstart.md (5 manual scenarios incl. refuse-or-flag probe + perf measurement table). Agent context updated.

## Outcome

- ✅ Impact: Full design for the one-ask pipeline; both live-test defects (silent 281 s scan, slot-template mismatch) have designed fixes; citation-grade invariant carried as "model proposes placement, deterministic code owns content".
- 🧪 Tests: planned gates listed; knn_parity is the R1 risk gate; acceptance runs on canned writer output (no live LLM in CI).
- 📁 Files: research.md, plan.md, data-model.md, contracts/one-ask-commands.md, quickstart.md, CLAUDE.md (auto).
- 🔁 Next prompts: /sp.tasks (phase D1 retrieval+KNN fix, D2 cards+skeleton, D3 compose+verify with canned fixtures, D4 commands+events, D5 frontend, D6 live 7B validation); ADR for the composition architecture.
- 🧠 Reflection: The KNN gap is bigger than this feature — query_router chat retrieval brute-forces too; flagged for a separate fix so 010 stays smallest-viable.

## Evaluation notes (flywheel)

- Failure modes observed: none new this stage; design defends against known ones (model emits numbers → R3; grammar crash → R2; silent waits → R8).
- Graders run and results (PASS/FAIL): constitution 13-principle check — PASS.
- Prompt variant (if applicable): n/a
- Next experiment (smallest change to try): implement knn_parity first — if vec0 MATCH/k ranking diverges from brute-force cosine on the fixture corpus, fall back to MATCH over-fetch + in-Rust cosine re-rank before any other task builds on retrieval.rs.
